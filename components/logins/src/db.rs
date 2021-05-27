/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Logins DB handling
///
/// The logins database works differently than other components because "mirror" and "local" mean
/// different things.  At some point we should probably refactor to make it match them, but here's
/// how it works for now:
///
///   - loginsM is the mirror table, which means it stores what we believe is on the server.  This
///     means either the last record we fetched from the server or the last record we uploaded.
///   - loginsL is the local table, which means it stores local changes that have not been sent to
///     the server.
///   - When we want to fetch a record, we need to look in both loginsL and loginsM for the data.
///     If a record is in both tables, then we prefer the loginsL data.  GET_BY_GUID_SQL contains a
///     clever UNION query to accomplish this.
///   - If a record is in both the local and mirror tables, we call the local record the "overlay"
///     and set the is_overridden flag on the mirror record.
///   - When we sync, the presence of a record in loginsL means that there was a local change that
///     we need to send to the the server and/or reconcile it with incoming changes from the
///     server.
///   - After we sync, we move all records from loginsL to loginsM, overwriting any previous data.
///     loginsL will be an empty table after this.  See mark_as_synchronized() for the details.
use crate::error::*;
use crate::login::{Login, SyncStatus};
use crate::migrate_sqlcipher_db::migrate_sqlcipher_db_to_plaintext;
use crate::schema;
use crate::util;
use lazy_static::lazy_static;
use rusqlite::{
    named_params,
    types::{FromSql, ToSql},
    Connection, NO_PARAMS,
};
use serde_derive::*;
use sql_support::{self, ConnExt};
use sql_support::{SqlInterruptHandle, SqlInterruptScope};
use std::ops::Deref;
use std::path::Path;
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::{Duration, Instant, SystemTime};
use sync_guid::Guid;
use url::{Host, Url};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct MigrationPhaseMetrics {
    pub num_processed: u64,
    pub num_succeeded: u64,
    pub num_failed: u64,
    pub total_duration: u128,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct MigrationMetrics {
    pub fixup_phase: MigrationPhaseMetrics,
    pub insert_phase: MigrationPhaseMetrics,
    pub num_processed: u64,
    pub num_succeeded: u64,
    pub num_failed: u64,
    pub total_duration: u128,
    pub errors: Vec<String>,
}

pub struct LoginDb {
    pub db: Connection,
    interrupt_counter: Arc<AtomicUsize>,
}

impl LoginDb {
    pub fn with_connection(db: Connection) -> Result<Self> {
        #[cfg(test)]
        {
            util::init_test_logging();
        }

        // `temp_store = 2` is required on Android to force the DB to keep temp
        // files in memory, since on Android there's no tmp partition. See
        // https://github.com/mozilla/mentat/issues/505. Ideally we'd only
        // do this on Android, or allow caller to configure it.
        db.set_pragma("temp_store", 2)?;

        let mut logins = Self {
            db,
            interrupt_counter: Arc::new(AtomicUsize::new(0)),
        };
        let tx = logins.db.transaction()?;
        schema::init(&tx)?;
        tx.commit()?;
        Ok(logins)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_connection(Connection::open(path)?)
    }

    // Open a dabase, after potentially migrating from a sqlcipher database.  This method handles
    // the migration process:
    //
    //    - If there's not a file at sqlcipher_path, then we skip the migratnion
    //    - If there is a file, then we attempt the migration and delete the file afterwards.
    //
    //  The salt arg is for IOS and other systems where the salt is stored externally.
    //
    pub fn open_with_sqlcipher_migration(
        path: impl AsRef<Path>,
        new_encryption_key: &str,
        sqlcipher_path: impl AsRef<Path>,
        sqlcipher_key: &str,
        salt: Option<&str>,
    ) -> Result<(Self, MigrationMetrics)> {
        let path = path.as_ref();
        let sqlcipher_path = sqlcipher_path.as_ref();

        let metrics = if sqlcipher_path.exists() {
            log::info!(
                "Migrating sqlcipher DB: {} -> {}",
                sqlcipher_path.display(),
                path.display()
            );
            let result = migrate_sqlcipher_db_to_plaintext(
                &sqlcipher_path,
                &path,
                sqlcipher_key,
                new_encryption_key,
                salt,
            );

            match result {
                Err(e) => {
                    log::error!("Error migrating sqlcipher DB: {}", e);
                    // Delete both the old and new paths (if they exist)
                    log::warn!("Re-creating database from scratch");
                    if sqlcipher_path.exists() {
                        std::fs::remove_file(sqlcipher_path)?;
                    }
                    if path.exists() {
                        std::fs::remove_file(&path)?;
                    }
                    MigrationMetrics { ..Default::default()}
                }
                Ok(metrics) => {
                    log::info!("Deleting old sqlcipher DB after migration");
                    if sqlcipher_path.exists() {
                        std::fs::remove_file(sqlcipher_path)?;
                    }
                    metrics
                }
            }
        } else {
            log::debug!("SQLCipher DB not found, skipping migration");
            MigrationMetrics { ..Default::default()}
        };

        Self::with_connection(Connection::open(&path)?).map(|db| (db, metrics))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    pub fn new_interrupt_handle(&self) -> SqlInterruptHandle {
        SqlInterruptHandle::new(
            self.db.get_interrupt_handle(),
            self.interrupt_counter.clone(),
        )
    }

    #[inline]
    pub fn begin_interrupt_scope(&self) -> SqlInterruptScope {
        SqlInterruptScope::new(self.interrupt_counter.clone())
    }
}

impl ConnExt for LoginDb {
    #[inline]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

impl Deref for LoginDb {
    type Target = Connection;
    #[inline]
    fn deref(&self) -> &Connection {
        &self.db
    }
}

// login specific stuff.

impl LoginDb {
    pub(crate) fn put_meta(&self, key: &str, value: &dyn ToSql) -> Result<()> {
        self.execute_named_cached(
            "REPLACE INTO loginsSyncMeta (key, value) VALUES (:key, :value)",
            named_params! { ":key": key, ":value": value },
        )?;
        Ok(())
    }

    pub(crate) fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        self.try_query_row(
            "SELECT value FROM loginsSyncMeta WHERE key = :key",
            named_params! { ":key": key },
            |row| Ok::<_, Error>(row.get(0)?),
            true,
        )
    }

    pub(crate) fn delete_meta(&self, key: &str) -> Result<()> {
        self.execute_named_cached(
            "DELETE FROM loginsSyncMeta WHERE key = :key",
            named_params! { ":key": key },
        )?;
        Ok(())
    }

    // It would be nice if this were a batch-ish api (e.g. takes a slice of records and finds dupes
    // for each one if they exist)... I can't think of how to write that query, though.
    // NOTE: currently used only by sync - maybe it should move to the sync engine?
    // It doesn't *feel* sync specific though?
    pub(crate) fn find_dupe(&self, l: &Login) -> Result<Option<Login>> {
        let form_submit_host_port = l
            .form_submit_url
            .as_ref()
            .and_then(|s| util::url_host_port(&s));
        let args = named_params! {
            ":hostname": l.hostname,
            ":http_realm": l.http_realm,
            ":username": l.username_enc,
            ":form_submit": form_submit_host_port,
        };
        let mut query = format!(
            "SELECT {common}
             FROM loginsL
             WHERE hostname IS :hostname
               AND httpRealm IS :http_realm
               AND username IS :username",
            common = schema::COMMON_COLS,
        );
        if form_submit_host_port.is_some() {
            // Stolen from iOS
            query += " AND (formSubmitURL = '' OR (instr(formSubmitURL, :form_submit) > 0))";
        } else {
            query += " AND formSubmitURL IS :form_submit"
        }
        self.try_query_row(&query, args, |row| Login::from_row(row), false)
    }

    pub fn get_all(&self) -> Result<Vec<Login>> {
        let mut stmt = self.db.prepare_cached(&GET_ALL_SQL)?;
        let rows = stmt.query_and_then(NO_PARAMS, Login::from_row)?;
        rows.collect::<Result<_>>()
    }

    pub fn get_by_base_domain(&self, base_domain: &str) -> Result<Vec<Login>> {
        // We first parse the input string as a host so it is normalized.
        let base_host = match Host::parse(base_domain) {
            Ok(d) => d,
            Err(e) => {
                // don't log the input string as it's PII.
                log::warn!("get_by_base_domain was passed an invalid domain: {}", e);
                return Ok(vec![]);
            }
        };
        // We just do a linear scan. Another option is to have an indexed
        // reverse-host column or similar, but current thinking is that it's
        // extra complexity for (probably) zero actual benefit given the record
        // counts are expected to be so low.
        // A regex would probably make this simpler, but we don't want to drag
        // in a regex lib just for this.
        let mut stmt = self.db.prepare_cached(&GET_ALL_SQL)?;
        let rows = stmt
            .query_and_then(NO_PARAMS, Login::from_row)?
            .filter(|r| {
                let login = r
                    .as_ref()
                    .ok()
                    .and_then(|login| Url::parse(&login.hostname).ok());
                let this_host = login.as_ref().and_then(|url| url.host());
                match (&base_host, this_host) {
                    (Host::Domain(base), Some(Host::Domain(look))) => {
                        // a fairly long-winded way of saying
                        // `login.hostname == base_domain ||
                        //  login.hostname.ends_with('.' + base_domain);`
                        let mut rev_input = base.chars().rev();
                        let mut rev_host = look.chars().rev();
                        loop {
                            match (rev_input.next(), rev_host.next()) {
                                (Some(ref a), Some(ref b)) if a == b => continue,
                                (None, None) => return true, // exactly equal
                                (None, Some(ref h)) => return *h == '.',
                                _ => return false,
                            }
                        }
                    }
                    // ip addresses must match exactly.
                    (Host::Ipv4(base), Some(Host::Ipv4(look))) => *base == look,
                    (Host::Ipv6(base), Some(Host::Ipv6(look))) => *base == look,
                    // all "mismatches" in domain types are false.
                    _ => false,
                }
            });
        rows.collect::<Result<_>>()
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Login>> {
        self.try_query_row(
            &GET_BY_GUID_SQL,
            &[(":guid", &id as &dyn ToSql)],
            Login::from_row,
            true,
        )
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        let tx = self.unchecked_transaction()?;
        self.ensure_local_overlay_exists(id)?;
        self.mark_mirror_overridden(id)?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        // As on iOS, just using a record doesn't flip it's status to changed.
        // TODO: this might be wrong for lockbox!
        self.execute_named_cached(
            "UPDATE loginsL
             SET timeLastUsed = :now_millis,
                 timesUsed = timesUsed + 1,
                 local_modified = :now_millis
             WHERE guid = :guid
                 AND is_deleted = 0",
            named_params! {
                ":now_millis": now_ms,
                ":guid": id,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn add(&self, login: Login) -> Result<Login> {
        let mut login = self.fixup_and_check_for_dupes(login)?;

        let tx = self.unchecked_transaction()?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());

        // Allow an empty GUID to be passed to indicate that we should generate
        // one. (Note that the FFI, does not require that the `id` field be
        // present in the JSON, and replaces it with an empty string if missing).
        if login.guid.is_empty() {
            login.guid = Guid::random()
        }

        // Fill in default metadata.
        if login.time_created == 0 {
            login.time_created = now_ms;
        }
        if login.time_password_changed == 0 {
            login.time_password_changed = now_ms;
        }
        if login.time_last_used == 0 {
            login.time_last_used = now_ms;
        }
        if login.times_used == 0 {
            login.times_used = 1;
        }

        let sql = format!(
            "INSERT OR IGNORE INTO loginsL (
                hostname,
                httpRealm,
                formSubmitURL,
                usernameField,
                passwordField,
                timesUsed,
                usernameEnc,
                passwordEnc,
                guid,
                timeCreated,
                timeLastUsed,
                timePasswordChanged,
                local_modified,
                is_deleted,
                sync_status
            ) VALUES (
                :hostname,
                :http_realm,
                :form_submit_url,
                :username_field,
                :password_field,
                :times_used,
                :username_enc,
                :password_enc,
                :guid,
                :time_created,
                :time_last_used,
                :time_password_changed,
                :local_modified,
                0, -- is_deleted
                {new} -- sync_status
            )",
            new = SyncStatus::New as u8
        );

        let rows_changed = self.execute_named(
            &sql,
            named_params! {
                ":hostname": login.hostname,
                ":http_realm": login.http_realm,
                ":form_submit_url": login.form_submit_url,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":username_enc": login.username_enc,
                ":password_enc": login.password_enc,
                ":guid": login.guid,
                ":time_created": login.time_created,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                ":local_modified": now_ms,
            },
        )?;
        if rows_changed == 0 {
            log::error!(
                "Record {:?} already exists (use `update` to update records, not add)",
                login.guid
            );
            throw!(ErrorKind::DuplicateGuid(login.guid.into_string()));
        }
        tx.commit()?;
        Ok(login)
    }

    pub fn import_multiple(&self, logins: &[Login]) -> Result<MigrationMetrics> {
        // Check if the logins table is empty first.
        let mut num_existing_logins =
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsL", NO_PARAMS, |r| r.get(0))?;
        num_existing_logins +=
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsM", NO_PARAMS, |r| r.get(0))?;
        if num_existing_logins > 0 {
            return Err(ErrorKind::NonEmptyTable.into());
        }
        let tx = self.unchecked_transaction()?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        let import_start = Instant::now();
        let sql = format!(
            "INSERT OR IGNORE INTO loginsL (
                hostname,
                httpRealm,
                formSubmitURL,
                usernameField,
                passwordField,
                timesUsed,
                usernameEnc,
                passwordEnc,
                guid,
                timeCreated,
                timeLastUsed,
                timePasswordChanged,
                local_modified,
                is_deleted,
                sync_status
            ) VALUES (
                :hostname,
                :http_realm,
                :form_submit_url,
                :username_field,
                :password_field,
                :times_used,
                :username_enc,
                :password_enc,
                :guid,
                :time_created,
                :time_last_used,
                :time_password_changed,
                :local_modified,
                0, -- is_deleted
                {new} -- sync_status
            )",
            new = SyncStatus::New as u8
        );
        let import_start_total_logins: u64 = logins.len() as u64;
        let mut num_failed_fixup: u64 = 0;
        let mut num_failed_insert: u64 = 0;
        let mut fixup_phase_duration = Duration::new(0, 0);
        let mut fixup_errors: Vec<String> = Vec::new();
        let mut insert_errors: Vec<String> = Vec::new();

        for login in logins {
            // This is a little bit of hoop-jumping to avoid cloning each borrowed item
            // in order to *possibly* created a fixed-up version.
            let mut login = login;
            let maybe_fixed_login = login.maybe_fixup().and_then(|fixed| {
                match &fixed {
                    None => self.check_for_dupes(login)?,
                    Some(l) => self.check_for_dupes(&l)?,
                };
                Ok(fixed)
            });
            match &maybe_fixed_login {
                Ok(None) => {} // The provided login was fine all along
                Ok(Some(l)) => {
                    // We made a new, fixed-up Login.
                    login = l;
                }
                Err(e) => {
                    log::warn!("Skipping login {} as it is invalid ({}).", login.guid, e);
                    fixup_errors.push(e.label().into());
                    num_failed_fixup += 1;
                    continue;
                }
            };
            // Now we can safely insert it, knowing that it's valid data.
            let old_guid = &login.guid; // Keep the old GUID around so we can debug errors easily.
            let guid = if old_guid.is_valid_for_sync_server() {
                old_guid.clone()
            } else {
                Guid::random()
            };
            fixup_phase_duration = import_start.elapsed();
            match self.execute_named_cached(
                &sql,
                named_params! {
                    ":hostname": login.hostname,
                    ":http_realm": login.http_realm,
                    ":form_submit_url": login.form_submit_url,
                    ":username_field": login.username_field,
                    ":password_field": login.password_field,
                    ":username_enc": login.username_enc,
                    ":password_enc": login.password_enc,
                    ":guid": guid,
                    ":time_created": login.time_created,
                    ":times_used": login.times_used,
                    ":time_last_used": login.time_last_used,
                    ":time_password_changed": login.time_password_changed,
                    ":local_modified": now_ms,
                },
            ) {
                Ok(_) => log::info!("Imported {} (new GUID {}) successfully.", old_guid, guid),
                Err(e) => {
                    log::warn!("Could not import {} ({}).", old_guid, e);
                    insert_errors.push(Error::from(e).label().into());
                    num_failed_insert += 1;
                }
            };
        }
        tx.commit()?;

        let num_post_fixup = import_start_total_logins - num_failed_fixup;
        let num_failed = num_failed_fixup + num_failed_insert;
        let insert_phase_duration = import_start
            .elapsed()
            .checked_sub(fixup_phase_duration)
            .unwrap_or_else(|| Duration::new(0, 0));
        let mut all_errors = Vec::new();
        all_errors.extend(fixup_errors.clone());
        all_errors.extend(insert_errors.clone());

        let metrics = MigrationMetrics {
            fixup_phase: MigrationPhaseMetrics {
                num_processed: import_start_total_logins,
                num_succeeded: num_post_fixup,
                num_failed: num_failed_fixup,
                total_duration: fixup_phase_duration.as_millis(),
                errors: fixup_errors,
            },
            insert_phase: MigrationPhaseMetrics {
                num_processed: num_post_fixup,
                num_succeeded: num_post_fixup - num_failed_insert,
                num_failed: num_failed_insert,
                total_duration: insert_phase_duration.as_millis(),
                errors: insert_errors,
            },
            num_processed: import_start_total_logins,
            num_succeeded: import_start_total_logins - num_failed,
            num_failed,
            total_duration: fixup_phase_duration
                .checked_add(insert_phase_duration)
                .unwrap_or_else(|| Duration::new(0, 0))
                .as_millis(),
            errors: all_errors,
        };
        log::info!(
            "Finished importing logins with the following metrics: {:#?}",
            metrics
        );
        Ok(metrics)
    }

    pub fn update(&self, login: Login) -> Result<()> {
        let login = self.fixup_and_check_for_dupes(login)?;

        let tx = self.unchecked_transaction()?;
        // Note: These fail with DuplicateGuid if the record doesn't exist.
        self.ensure_local_overlay_exists(login.guid_str())?;
        self.mark_mirror_overridden(login.guid_str())?;

        let now_ms = util::system_time_ms_i64(SystemTime::now());

        // TODO-sqlcipher: consider changing the timePasswordChanged comparison code (SYNC-2197)
        let sql = format!(
            "UPDATE loginsL
             SET local_modified      = :now_millis,
                 timeLastUsed        = :now_millis,
                 -- Only update timePasswordChanged if, well, the password changed.
                 timePasswordChanged = (CASE
                     WHEN passwordEnc = :passwordEnc
                     THEN timePasswordChanged
                     ELSE :now_millis
                 END),
                 httpRealm           = :http_realm,
                 formSubmitURL       = :form_submit_url,
                 usernameField       = :username_field,
                 passwordField       = :password_field,
                 timesUsed           = timesUsed + 1,
                 usernameEnc         = :username_enc,
                 passwordEnc         = :password_enc,
                 hostname            = :hostname,
                 -- leave New records as they are, otherwise update them to `changed`
                 sync_status         = max(sync_status, {changed})
             WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );

        self.db.execute_named(
            &sql,
            named_params! {
                ":hostname": login.hostname,
                ":username_enc": login.username_enc,
                ":password_enc": login.password_enc,
                ":http_realm": login.http_realm,
                ":form_submit_url": login.form_submit_url,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":guid": login.guid,
                ":now_millis": now_ms,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn check_valid_with_no_dupes(&self, login: &Login) -> Result<()> {
        login.check_valid()?;
        self.check_for_dupes(login)
    }

    pub fn fixup_and_check_for_dupes(&self, login: Login) -> Result<Login> {
        let login = login.fixup()?;
        self.check_for_dupes(&login)?;
        Ok(login)
    }

    pub fn check_for_dupes(&self, login: &Login) -> Result<()> {
        if self.dupe_exists(&login)? {
            throw!(InvalidLogin::DuplicateLogin);
        }
        Ok(())
    }

    pub fn dupe_exists(&self, _login: &Login) -> Result<bool> {
        // TODO-sqlcipher: Need to update this one to work with encrypted usernames -- this is probably going
        // to happen in conjunction with updating the API shape, so let's punt on it for now
        Ok(false)
        // // Note: the query below compares the guids of the given login with existing logins
        // //  to prevent a login from being considered a duplicate of itself (e.g. during updates).
        // Ok(self.db.query_row_named(
        //     "SELECT EXISTS(
        //         SELECT 1 FROM loginsL
        //         WHERE is_deleted = 0
        //             AND guid <> :guid
        //             AND hostname = :hostname
        //             AND NULLIF(username, '') = :username
        //             AND (
        //                 formSubmitURL = :form_submit
        //                 OR
        //                 httpRealm = :http_realm
        //             )
        //
        //         UNION ALL
        //
        //         SELECT 1 FROM loginsM
        //         WHERE is_overridden = 0
        //             AND guid <> :guid
        //             AND hostname = :hostname
        //             AND NULLIF(username, '') = :username
        //             AND (
        //                 formSubmitURL = :form_submit
        //                 OR
        //                 httpRealm = :http_realm
        //             )
        //      )",
        //     named_params! {
        //         ":guid": &login.guid,
        //         ":hostname": &login.hostname,
        //         ":username": &login.username_enc,
        //         ":http_realm": login.http_realm.as_ref(),
        //         ":form_submit": login.form_submit_url.as_ref(),
        //     },
        //     |row| row.get(0),
        // )?)
    }

    pub fn potential_dupes_ignoring_username(&self, login: &Login) -> Result<Vec<Login>> {
        // Could be lazy_static-ed...
        lazy_static::lazy_static! {
            static ref DUPES_IGNORING_USERNAME_SQL: String = format!(
                "SELECT {common_cols} FROM loginsL
                WHERE is_deleted = 0
                    AND hostname = :hostname
                    AND (
                        formSubmitURL = :form_submit
                        OR
                        httpRealm = :http_realm
                    )

                UNION ALL

                SELECT {common_cols} FROM loginsM
                WHERE is_overridden = 0
                    AND hostname = :hostname
                    AND (
                        formSubmitURL = :form_submit
                        OR
                        httpRealm = :http_realm
                    )
                ",
                common_cols = schema::COMMON_COLS
            );
        }
        let mut stmt = self.db.prepare_cached(&DUPES_IGNORING_USERNAME_SQL)?;
        let params = named_params! {
            ":hostname": &login.hostname,
            ":http_realm": login.http_realm.as_ref(),
            ":form_submit": login.form_submit_url.as_ref(),
        };
        // Needs to be two lines for borrow checker
        let rows = stmt.query_and_then_named(params, Login::from_row)?;
        rows.collect()
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row_named(
            "SELECT EXISTS(
                 SELECT 1 FROM loginsL
                 WHERE guid = :guid AND is_deleted = 0
                 UNION ALL
                 SELECT 1 FROM loginsM
                 WHERE guid = :guid AND is_overridden IS NOT 1
             )",
            named_params! { ":guid": id },
            |row| row.get(0),
        )?)
    }

    /// Delete the record with the provided id. Returns true if the record
    /// existed already.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let tx = self.unchecked_transaction_imm()?;
        let exists = self.exists(id)?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());

        // For IDs that have, mark is_deleted and clear sensitive fields
        self.execute_named(
            &format!(
                "UPDATE loginsL
                 SET local_modified = :now_ms,
                     sync_status = {status_changed},
                     is_deleted = 1,
                     passwordEnc = '',
                     hostname = '',
                     usernameEnc = ''
                 WHERE guid = :guid",
                status_changed = SyncStatus::Changed as u8
            ),
            named_params! { ":now_ms": now_ms, ":guid": id },
        )?;

        // Mark the mirror as overridden
        self.execute_named(
            "UPDATE loginsM SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": id },
        )?;

        // If we don't have a local record for this ID, but do have it in the mirror
        // insert a tombstone.
        self.execute_named(&format!("
            INSERT OR IGNORE INTO loginsL
                    (guid, local_modified, is_deleted, sync_status, hostname, timeCreated, timePasswordChanged, passwordEnc, usernameEnc)
            SELECT   guid, :now_ms,        1,          {changed},   '',       timeCreated, :now_ms,                   '',       ''
            FROM loginsM
            WHERE guid = :guid",
            changed = SyncStatus::Changed as u8),
            named_params! { ":now_ms": now_ms, ":guid": id })?;
        tx.commit()?;
        Ok(exists)
    }

    fn mark_mirror_overridden(&self, guid: &str) -> Result<()> {
        self.execute_named_cached(
            "UPDATE loginsM SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": guid },
        )?;
        Ok(())
    }

    fn ensure_local_overlay_exists(&self, guid: &str) -> Result<()> {
        let already_have_local: bool = self.db.query_row_named(
            "SELECT EXISTS(SELECT 1 FROM loginsL WHERE guid = :guid)",
            named_params! { ":guid": guid },
            |row| row.get(0),
        )?;

        if already_have_local {
            return Ok(());
        }

        log::debug!("No overlay; cloning one for {:?}.", guid);
        let changed = self.clone_mirror_to_overlay(guid)?;
        if changed == 0 {
            log::error!("Failed to create local overlay for GUID {:?}.", guid);
            throw!(ErrorKind::NoSuchRecord(guid.to_owned()));
        }
        Ok(())
    }

    fn clone_mirror_to_overlay(&self, guid: &str) -> Result<usize> {
        Ok(self
            .execute_named_cached(&*CLONE_SINGLE_MIRROR_SQL, &[(":guid", &guid as &dyn ToSql)])?)
    }

    // Wipe is called both by Sync and also exposed publically, so it's
    // implemented here.
    pub(crate) fn wipe(&self, scope: &SqlInterruptScope) -> Result<()> {
        let tx = self.unchecked_transaction()?;
        log::info!("Executing wipe on password engine!");
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        scope.err_if_interrupted()?;
        self.execute_named(
            &format!(
                "
                UPDATE loginsL
                SET local_modified = :now_ms,
                    sync_status = {changed},
                    is_deleted = 1,
                    passwordEnc = '',
                    hostname = '',
                    usernameEnc = ''
                WHERE is_deleted = 0",
                changed = SyncStatus::Changed as u8
            ),
            named_params! { ":now_ms": now_ms },
        )?;
        scope.err_if_interrupted()?;

        self.execute("UPDATE loginsM SET is_overridden = 1", NO_PARAMS)?;
        scope.err_if_interrupted()?;

        self.execute_named(
            &format!("
                INSERT OR IGNORE INTO loginsL
                      (guid, local_modified, is_deleted, sync_status, hostname, timeCreated, timePasswordChanged, passwordEnc, usernameEnc)
                SELECT guid, :now_ms,        1,          {changed},   '',       timeCreated, :now_ms,             '',       ''
                FROM loginsM",
                changed = SyncStatus::Changed as u8),
            named_params! { ":now_ms": now_ms })?;
        scope.err_if_interrupted()?;
        tx.commit()?;
        Ok(())
    }

    pub fn wipe_local(&self) -> Result<()> {
        log::info!("Executing wipe_local on password engine!");
        let tx = self.unchecked_transaction()?;
        self.execute_all(&[
            "DELETE FROM loginsL",
            "DELETE FROM loginsM",
            "DELETE FROM loginsSyncMeta",
        ])?;
        tx.commit()?;
        Ok(())
    }
}

lazy_static! {
    static ref GET_ALL_SQL: String = format!(
        "SELECT {common_cols} FROM loginsL WHERE is_deleted = 0
         UNION ALL
         SELECT {common_cols} FROM loginsM WHERE is_overridden = 0",
        common_cols = schema::COMMON_COLS,
    );
    static ref GET_BY_GUID_SQL: String = format!(
        "SELECT {common_cols}
         FROM loginsL
         WHERE is_deleted = 0
           AND guid = :guid

         UNION ALL

         SELECT {common_cols}
         FROM loginsM
         WHERE is_overridden IS NOT 1
           AND guid = :guid
         ORDER BY hostname ASC

         LIMIT 1",
        common_cols = schema::COMMON_COLS,
    );
    pub static ref CLONE_ENTIRE_MIRROR_SQL: String = format!(
        "INSERT OR IGNORE INTO loginsL ({common_cols}, local_modified, is_deleted, sync_status)
         SELECT {common_cols}, NULL AS local_modified, 0 AS is_deleted, 0 AS sync_status
         FROM loginsM",
        common_cols = schema::COMMON_COLS,
    );
    static ref CLONE_SINGLE_MIRROR_SQL: String =
        format!("{} WHERE guid = :guid", &*CLONE_ENTIRE_MIRROR_SQL,);
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::encryption::test_utils::decrypt;
    use crate::login::test_utils::login;
    use sync15::ServerTimestamp;

    // Insert a login into the local and/or mirror tables.
    //
    // local_login and mirror_login are specifed as Some(password_string)
    pub fn insert_login(
        db: &LoginDb,
        guid: &str,
        local_login: Option<&str>,
        mirror_login: Option<&str>,
    ) {
        if let Some(password) = mirror_login {
            add_mirror(
                &db,
                &login(guid, password),
                &ServerTimestamp(util::system_time_ms_i64(std::time::SystemTime::now())),
                local_login.is_some(),
            )
            .unwrap();
        }
        if let Some(password) = local_login {
            db.add(login(guid, password)).unwrap();
        }
    }

    pub fn add_mirror(
        db: &LoginDb,
        login: &Login,
        server_modified: &ServerTimestamp,
        is_overridden: bool,
    ) -> Result<()> {
        let sql = "
            INSERT OR IGNORE INTO loginsM (
                is_overridden,
                server_modified,

                httpRealm,
                formSubmitURL,
                usernameField,
                passwordField,
                passwordEnc,
                hostname,
                usernameEnc,

                timesUsed,
                timeLastUsed,
                timePasswordChanged,
                timeCreated,

                guid
            ) VALUES (
                :is_overridden,
                :server_modified,

                :http_realm,
                :form_submit_url,
                :username_field,
                :password_field,
                :password_enc,
                :hostname,
                :username_enc,

                :times_used,
                :time_last_used,
                :time_password_changed,
                :time_created,

                :guid
            )";
        let mut stmt = db.prepare_cached(&sql)?;

        stmt.execute_named(named_params! {
            ":is_overridden": is_overridden,
            ":server_modified": server_modified.as_millis(),
            ":http_realm": login.http_realm,
            ":form_submit_url": login.form_submit_url,
            ":username_field": login.username_field,
            ":password_field": login.password_field,
            ":password_enc": login.password_enc,
            ":hostname": login.hostname,
            ":username_enc": login.username_enc,
            ":times_used": login.times_used,
            ":time_last_used": login.time_last_used,
            ":time_password_changed": login.time_password_changed,
            ":time_created": login.time_created,
            ":guid": login.guid_str(),
        })?;
        Ok(())
    }

    pub fn get_local_guids(db: &LoginDb) -> Vec<String> {
        get_guids(db, "SELECT guid FROM loginsL")
    }

    pub fn get_mirror_guids(db: &LoginDb) -> Vec<String> {
        get_guids(db, "SELECT guid FROM loginsM")
    }

    fn get_guids(db: &LoginDb, sql: &str) -> Vec<String> {
        let mut stmt = db.prepare_cached(sql).unwrap();
        let mut res: Vec<String> = stmt
            .query_map(NO_PARAMS, |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        res.sort();
        res
    }

    pub fn get_server_modified(db: &LoginDb, guid: &str) -> i64 {
        db.query_one(&format!(
            "SELECT server_modified FROM loginsM WHERE guid='{}'",
            guid
        ))
        .unwrap()
    }

    pub fn check_local_login(db: &LoginDb, guid: &str, password: &str, local_modified_gte: i64) {
        let row: (String, i64, bool) = db
            .query_row(
                "SELECT passwordEnc, local_modified, is_deleted FROM loginsL WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(decrypt(&row.0), password);
        assert!(row.1 >= local_modified_gte);
        assert!(!row.2);
    }

    pub fn check_mirror_login(
        db: &LoginDb,
        guid: &str,
        password: &str,
        server_modified: i64,
        is_overridden: bool,
    ) {
        let row: (String, i64, bool) = db
            .query_row(
                "SELECT passwordEnc, server_modified, is_overridden FROM loginsM WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(decrypt(&row.0), password);
        assert_eq!(row.1, server_modified);
        assert_eq!(row.2, is_overridden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_utils::{decrypt, encrypt};

    // TODO-sqlcipher remove the ignore flag once we re-implement dupe checking
    #[test]
    #[ignore]
    fn test_check_valid_with_no_dupes() {
        let db = LoginDb::open_in_memory().unwrap();
        db.add(Login {
            guid: "dummy_000001".into(),
            form_submit_url: Some("https://www.example.com".into()),
            hostname: "https://www.example.com".into(),
            http_realm: None,
            username_enc: encrypt("test"),
            password_enc: encrypt("test"),
            ..Login::default()
        })
        .unwrap();

        let unique_login_guid = Guid::empty();
        let unique_login = Login {
            guid: unique_login_guid.clone(),
            form_submit_url: None,
            hostname: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            username_enc: encrypt("test"),
            password_enc: encrypt("test"),
            ..Login::default()
        };

        let duplicate_login = Login {
            guid: Guid::empty(),
            form_submit_url: Some("https://www.example.com".into()),
            hostname: "https://www.example.com".into(),
            http_realm: None,
            username_enc: encrypt("test"),
            password_enc: encrypt("test2"),
            ..Login::default()
        };

        let updated_login = Login {
            guid: unique_login_guid,
            form_submit_url: None,
            hostname: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            username_enc: encrypt("test"),
            password_enc: encrypt("test4"),
            ..Login::default()
        };

        struct TestCase {
            login: Login,
            should_err: bool,
            expected_err: &'static str,
        }

        let test_cases = [
            TestCase {
                // unique_login should not error because it does not share the same hostname,
                // username, and formSubmitURL or httpRealm with the pre-existing login
                // (login with guid "dummy_000001").
                login: unique_login,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                // duplicate_login has the same hostname, username, and formSubmitURL as a pre-existing
                // login (guid "dummy_000001") and duplicate_login has no guid value, i.e. its guid
                // doesn't match with that of a pre-existing record so it can't be considered update,
                // so it should error.
                login: duplicate_login,
                should_err: true,
                expected_err: "Invalid login: Login already exists",
            },
            TestCase {
                // updated_login is an update to unique_login (has the same guid) so it is not a dupe
                // and should not error.
                login: updated_login,
                should_err: false,
                expected_err: "",
            },
        ];

        for tc in &test_cases {
            let login_check = db.check_valid_with_no_dupes(&tc.login);
            if tc.should_err {
                assert!(&login_check.is_err());
                assert_eq!(&login_check.unwrap_err().to_string(), tc.expected_err)
            } else {
                assert!(&login_check.is_ok())
            }
        }
    }

    #[test]
    fn test_unicode_submit() {
        let db = LoginDb::open_in_memory().unwrap();
        db.add(Login {
            guid: "dummy_000001".into(),
            form_submit_url: Some("http://😍.com".into()),
            hostname: "http://😍.com".into(),
            http_realm: None,
            username_enc: encrypt("😍"),
            username_field: "😍".into(),
            password_enc: encrypt("😍"),
            password_field: "😍".into(),
            ..Login::default()
        })
        .unwrap();
        let fetched = db
            .get_by_id("dummy_000001")
            .expect("should work")
            .expect("should get a record");
        assert_eq!(fetched.hostname, "http://xn--r28h.com");
        assert_eq!(fetched.form_submit_url.unwrap(), "http://xn--r28h.com");
        assert_eq!(decrypt(&fetched.username_enc), "😍");
        assert_eq!(fetched.username_field, "😍");
        assert_eq!(decrypt(&fetched.password_enc), "😍");
        assert_eq!(fetched.password_field, "😍");
    }

    #[test]
    fn test_unicode_realm() {
        let db = LoginDb::open_in_memory().unwrap();
        db.add(Login {
            guid: "dummy_000001".into(),
            form_submit_url: None,
            hostname: "http://😍.com".into(),
            http_realm: Some("😍😍".into()),
            username_enc: encrypt("😍"),
            password_enc: encrypt("😍"),
            ..Login::default()
        })
        .unwrap();
        let fetched = db
            .get_by_id("dummy_000001")
            .expect("should work")
            .expect("should get a record");
        assert_eq!(fetched.hostname, "http://xn--r28h.com");
        assert_eq!(fetched.http_realm.unwrap(), "😍😍");
    }

    fn check_matches(db: &LoginDb, query: &str, expected: &[&str]) {
        let mut results = db
            .get_by_base_domain(query)
            .unwrap()
            .into_iter()
            .map(|l| l.hostname)
            .collect::<Vec<String>>();
        results.sort_unstable();
        let mut sorted = expected.to_owned();
        sorted.sort_unstable();
        assert_eq!(sorted, results);
    }

    fn check_good_bad(
        good: Vec<&str>,
        bad: Vec<&str>,
        good_queries: Vec<&str>,
        zero_queries: Vec<&str>,
    ) {
        let db = LoginDb::open_in_memory().unwrap();
        for h in good.iter().chain(bad.iter()) {
            db.add(Login {
                hostname: (*h).into(),
                http_realm: Some((*h).into()),
                password_enc: encrypt("test"),
                ..Login::default()
            })
            .unwrap();
        }
        for query in good_queries {
            check_matches(&db, query, &good);
        }
        for query in zero_queries {
            check_matches(&db, query, &[]);
        }
    }

    #[test]
    fn test_get_by_base_domain_invalid() {
        check_good_bad(
            vec!["https://example.com"],
            vec![],
            vec![],
            vec!["invalid query"],
        );
    }

    #[test]
    fn test_get_by_base_domain() {
        check_good_bad(
            vec![
                "https://example.com",
                "https://www.example.com",
                "http://www.example.com",
                "http://www.example.com:8080",
                "http://sub.example.com:8080",
                "https://sub.example.com:8080",
                "https://sub.sub.example.com",
                "ftp://sub.example.com",
            ],
            vec![
                "https://badexample.com",
                "https://example.co",
                "https://example.com.au",
            ],
            vec!["example.com"],
            vec!["foo.com"],
        );
        // punycode! This is likely to need adjusting once we normalize
        // on insert.
        check_good_bad(
            vec![
                "http://xn--r28h.com", // punycoded version of "http://😍.com"
            ],
            vec!["http://💖.com"],
            vec!["😍.com", "xn--r28h.com"],
            vec![],
        );
    }

    #[test]
    fn test_get_by_base_domain_ipv4() {
        check_good_bad(
            vec!["http://127.0.0.1", "https://127.0.0.1:8000"],
            vec!["https://127.0.0.0", "https://example.com"],
            vec!["127.0.0.1"],
            vec!["127.0.0.2"],
        );
    }

    #[test]
    fn test_get_by_base_domain_ipv6() {
        check_good_bad(
            vec!["http://[::1]", "https://[::1]:8000"],
            vec!["https://[0:0:0:0:0:0:1:1]", "https://example.com"],
            vec!["[::1]", "[0:0:0:0:0:0:0:1]"],
            vec!["[0:0:0:0:0:0:1:2]"],
        );
    }

    #[test]
    fn test_add() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = Login {
            hostname: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            username_enc: encrypt("test_user"),
            password_enc: encrypt("test_password"),
            guid: Guid::new("a"),
            ..Login::default()
        };
        db.add(login.clone()).unwrap();
        let login2 = db.get_by_id("a").unwrap().unwrap();

        assert_eq!(login.hostname, login2.hostname);
        assert_eq!(login.http_realm, login2.http_realm);
        assert_eq!(login.username_enc, login2.username_enc);
        assert_eq!(login.password_enc, login2.password_enc);
    }

    #[test]
    fn test_update() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(Login {
                hostname: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                username_enc: encrypt("user1"),
                password_enc: encrypt("password1"),
                guid: Guid::new("a"),
                ..Login::default()
            })
            .unwrap();
        db.update(Login {
            hostname: "https://www.example2.com".into(),
            http_realm: Some("https://www.example2.com".into()),
            username_enc: encrypt("user2"),
            password_enc: encrypt("password2"),
            ..login
        })
        .unwrap();

        let login2 = db.get_by_id("a").unwrap().unwrap();

        assert_eq!(login2.hostname, "https://www.example2.com");
        assert_eq!(login2.http_realm, Some("https://www.example2.com".into()));
        assert_eq!(decrypt(&login2.username_enc), "user2");
        assert_eq!(decrypt(&login2.password_enc), "password2");
    }

    #[test]
    fn test_touch() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(Login {
                hostname: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                username_enc: encrypt("user1"),
                password_enc: encrypt("password1"),
                guid: Guid::new("a"),
                ..Login::default()
            })
            .unwrap();
        db.touch(&"a").unwrap();
        let login2 = db.get_by_id("a").unwrap().unwrap();
        assert!(login2.time_last_used > login.time_last_used);
        assert_eq!(login2.times_used, login.times_used + 1);
    }

    #[test]
    fn test_delete() {
        let db = LoginDb::open_in_memory().unwrap();
        let _login = db
            .add(Login {
                hostname: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                username_enc: encrypt("test_user"),
                password_enc: encrypt("test_password"),
                ..Login::default()
            })
            .unwrap();

        assert!(db.delete(_login.guid_str()).unwrap());

        let tombstone_exists: bool = db
            .query_row_named(
                "SELECT EXISTS(
                    SELECT 1 FROM loginsL
                    WHERE guid = :guid AND is_deleted = 1
                )",
                named_params! { ":guid": _login.guid_str() },
                |row| row.get(0),
            )
            .unwrap();

        assert!(tombstone_exists);
        assert!(!db.exists(_login.guid_str()).unwrap());
    }

    #[test]
    fn test_wipe() {
        let db = LoginDb::open_in_memory().unwrap();
        let login1 = db
            .add(Login {
                hostname: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                username_enc: encrypt("test_user_1"),
                password_enc: encrypt("test_password_1"),
                ..Login::default()
            })
            .unwrap();

        let login2 = db
            .add(Login {
                hostname: "https://www.example2.com".into(),
                http_realm: Some("https://www.example2.com".into()),
                username_enc: encrypt("test_user_1"),
                password_enc: encrypt("test_password_2"),
                ..Login::default()
            })
            .unwrap();

        assert!(db.wipe(&db.begin_interrupt_scope()).is_ok());

        let expected_tombstone_count = 2;
        let actual_tombstone_count: i32 = db
            .query_row_named(
                "SELECT COUNT(guid)
                    FROM loginsL
                    WHERE guid IN (:guid1,:guid2)
                        AND is_deleted = 1",
                named_params! {
                    ":guid1": login1.guid_str(),
                    ":guid2": login2.guid_str(),
                },
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(expected_tombstone_count, actual_tombstone_count);
        assert!(!db.exists(login1.guid_str()).unwrap());
        assert!(!db.exists(login2.guid_str()).unwrap());
    }

    fn delete_logins(db: &LoginDb, guids: &[String]) -> Result<()> {
        sql_support::each_chunk(guids, |chunk, _| -> Result<()> {
            db.execute(
                &format!(
                    "DELETE FROM loginsL WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn test_import_multiple() {
        struct TestCase {
            logins: Vec<Login>,
            has_populated_metrics: bool,
            expected_metrics: MigrationMetrics,
        }

        let db = LoginDb::open_in_memory().unwrap();

        // Adding login to trigger non-empty table error
        let login = db
            .add(Login {
                hostname: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                username_enc: encrypt("test_user_1"),
                password_enc: encrypt("test_password_1"),
                ..Login::default()
            })
            .unwrap();

        let import_with_populated_table = db.import_multiple(Vec::new().as_slice());
        assert!(import_with_populated_table.is_err());
        assert_eq!(
            import_with_populated_table.unwrap_err().to_string(),
            "The logins tables are not empty"
        );

        // Removing added login so the test cases below don't fail
        delete_logins(&db, &[login.guid.into_string()]).unwrap();

        // Setting up test cases
        let valid_login_guid1: Guid = Guid::random();
        let valid_login1 = Login {
            guid: valid_login_guid1,
            form_submit_url: Some("https://www.example.com".into()),
            hostname: "https://www.example.com".into(),
            http_realm: None,
            username_enc: encrypt("test"),
            password_enc: encrypt("test"),
            ..Login::default()
        };
        let valid_login_guid2: Guid = Guid::random();
        let valid_login2 = Login {
            guid: valid_login_guid2,
            form_submit_url: Some("https://www.example2.com".into()),
            hostname: "https://www.example2.com".into(),
            http_realm: None,
            username_enc: encrypt("test2"),
            password_enc: encrypt("test2"),
            ..Login::default()
        };
        let valid_login_guid3: Guid = Guid::random();
        let valid_login3 = Login {
            guid: valid_login_guid3,
            form_submit_url: Some("https://www.example3.com".into()),
            hostname: "https://www.example3.com".into(),
            http_realm: None,
            username_enc: encrypt("test3"),
            password_enc: encrypt("test3"),
            ..Login::default()
        };
        let duplicate_login_guid: Guid = Guid::random();
        let duplicate_login = Login {
            guid: duplicate_login_guid,
            form_submit_url: Some("https://www.example.com".into()),
            hostname: "https://www.example.com".into(),
            http_realm: None,
            username_enc: encrypt("test"),
            password_enc: encrypt("test2"),
            ..Login::default()
        };

        let _duplicate_logins = vec![valid_login1.clone(), duplicate_login, valid_login2.clone()];

        let _duplicate_logins_metrics = MigrationMetrics {
            fixup_phase: MigrationPhaseMetrics {
                num_processed: 3,
                num_succeeded: 2,
                num_failed: 1,
                errors: vec!["InvalidLogin::DuplicateLogin".into()],
                ..MigrationPhaseMetrics::default()
            },
            insert_phase: MigrationPhaseMetrics {
                num_processed: 2,
                num_succeeded: 2,
                ..MigrationPhaseMetrics::default()
            },
            num_processed: 3,
            num_succeeded: 2,
            num_failed: 1,
            errors: vec!["InvalidLogin::DuplicateLogin".into()],
            ..MigrationMetrics::default()
        };

        let valid_logins = vec![valid_login1, valid_login2, valid_login3];

        let valid_logins_metrics = MigrationMetrics {
            fixup_phase: MigrationPhaseMetrics {
                num_processed: 3,
                num_succeeded: 3,
                ..MigrationPhaseMetrics::default()
            },
            insert_phase: MigrationPhaseMetrics {
                num_processed: 3,
                num_succeeded: 3,
                ..MigrationPhaseMetrics::default()
            },
            num_processed: 3,
            num_succeeded: 3,
            ..MigrationMetrics::default()
        };

        let test_cases = [
            TestCase {
                logins: Vec::new(),
                has_populated_metrics: false,
                expected_metrics: MigrationMetrics {
                    ..MigrationMetrics::default()
                },
            },
            // TODO-sqlcipher: Re-add this case once dupe checking is re-implemented
            // TestCase {
            //     logins: duplicate_logins,
            //     has_populated_metrics: true,
            //     expected_metrics: duplicate_logins_metrics,
            // },
            TestCase {
                logins: valid_logins,
                has_populated_metrics: true,
                expected_metrics: valid_logins_metrics,
            },
        ];

        for tc in &test_cases {
            let import_result = db.import_multiple(tc.logins.as_slice());
            assert!(import_result.is_ok());

            let mut actual_metrics = import_result.unwrap();

            if tc.has_populated_metrics {
                let mut guids = Vec::new();
                for login in &tc.logins {
                    guids.push(login.clone().guid.into_string());
                }

                assert_eq!(
                    actual_metrics.num_processed,
                    tc.expected_metrics.num_processed
                );
                assert_eq!(
                    actual_metrics.num_succeeded,
                    tc.expected_metrics.num_succeeded
                );
                assert_eq!(actual_metrics.num_failed, tc.expected_metrics.num_failed);
                assert_eq!(actual_metrics.errors, tc.expected_metrics.errors);

                let phases = [
                    (
                        actual_metrics.fixup_phase,
                        tc.expected_metrics.fixup_phase.clone(),
                    ),
                    (
                        actual_metrics.insert_phase,
                        tc.expected_metrics.insert_phase.clone(),
                    ),
                ];

                for (actual, expected) in &phases {
                    assert_eq!(actual.num_processed, expected.num_processed);
                    assert_eq!(actual.num_succeeded, expected.num_succeeded);
                    assert_eq!(actual.num_failed, expected.num_failed);
                    assert_eq!(actual.errors, expected.errors);
                }

                // clearing the database for next test case
                delete_logins(&db, guids.as_slice()).unwrap();
            } else {
                // We could elaborate mock out the clock for tests...
                // or we could just set the duration fields to the right values!
                actual_metrics.total_duration = tc.expected_metrics.total_duration;
                actual_metrics.fixup_phase.total_duration =
                    tc.expected_metrics.fixup_phase.total_duration;
                actual_metrics.insert_phase.total_duration =
                    tc.expected_metrics.insert_phase.total_duration;
                assert_eq!(actual_metrics, tc.expected_metrics);
            }
        }
    }
}
