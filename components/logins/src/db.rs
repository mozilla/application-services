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
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::login::{EncryptedFields, Login, LoginFields, UpdatableLogin, ValidateAndFixup};
use crate::migrate_sqlcipher_db::migrate_sqlcipher_db_to_plaintext;
use crate::schema;
use crate::sync::SyncStatus;
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
    pub(crate) num_processed: u64,
    pub(crate) num_succeeded: u64,
    pub(crate) num_failed: u64,
    pub(crate) total_duration: u64,
    pub(crate) errors: Vec<String>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct MigrationMetrics {
    pub(crate) fixup_phase: MigrationPhaseMetrics,
    pub(crate) insert_phase: MigrationPhaseMetrics,
    pub(crate) num_processed: u64,
    pub(crate) num_succeeded: u64,
    pub(crate) num_failed: u64,
    pub(crate) total_duration: u64,
    pub(crate) errors: Vec<String>,
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

    // Open a database, after potentially migrating from a sqlcipher database.  This method handles
    // the migration process:
    //
    //    - If there's not a file at sqlcipher_path, then we skip the migration
    //    - If there is a file, then we attempt the migration and delete the file afterwards.
    //
    //  The salt arg is for IOS where the salt is stored externally.
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
                    MigrationMetrics::default()
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
            MigrationMetrics::default()
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
                    .and_then(|login| Url::parse(&login.fields.origin).ok());
                let this_host = login.as_ref().and_then(|url| url.host());
                match (&base_host, this_host) {
                    (Host::Domain(base), Some(Host::Domain(look))) => {
                        // a fairly long-winded way of saying
                        // `fields.origin == base_domain ||
                        //  fields.origin.ends_with('.' + base_domain);`
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

    // The single place we insert new rows or update existing local rows.
    // just the SQL - no validation or anything.
    fn insert_new_login(&self, login: &Login) -> Result<()> {
        let sql = format!(
            "INSERT INTO loginsL (
                origin,
                httpRealm,
                formActionOrigin,
                usernameField,
                passwordField,
                timesUsed,
                encFields,
                guid,
                timeCreated,
                timeLastUsed,
                timePasswordChanged,
                local_modified,
                is_deleted,
                sync_status
            ) VALUES (
                :origin,
                :http_realm,
                :form_action_origin,
                :username_field,
                :password_field,
                :times_used,
                :enc_fields,
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

        self.execute_named(
            &sql,
            named_params! {
                ":origin": login.fields.origin,
                ":http_realm": login.fields.http_realm,
                ":form_action_origin": login.fields.form_action_origin,
                ":username_field": login.fields.username_field,
                ":password_field": login.fields.password_field,
                ":time_created": login.time_created,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                ":local_modified": login.time_created,
                ":enc_fields": login.enc_fields,
                ":guid": login.guid(),
            },
        )?;
        Ok(())
    }

    fn update_existing_login(&self, login: &Login) -> Result<()> {
        // assumes the "local overlay" exists, so the guid must too.
        let sql = format!(
            "UPDATE loginsL
             SET local_modified      = :now_millis,
                 timeLastUsed        = :time_last_used,
                 timePasswordChanged = :time_password_changed,
                 httpRealm           = :http_realm,
                 formActionOrigin    = :form_action_origin,
                 usernameField       = :username_field,
                 passwordField       = :password_field,
                 timesUsed           = :times_used,
                 encFields           = :enc_fields,
                 origin              = :origin,
                 -- leave New records as they are, otherwise update them to `changed`
                 sync_status         = max(sync_status, {changed})
             WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );

        self.db.execute_named(
            &sql,
            named_params! {
                ":origin": login.fields.origin,
                ":http_realm": login.fields.http_realm,
                ":form_action_origin": login.fields.form_action_origin,
                ":username_field": login.fields.username_field,
                ":password_field": login.fields.password_field,
                ":time_last_used": login.time_last_used,
                ":times_used": login.times_used,
                ":time_password_changed": login.time_password_changed,
                ":enc_fields": login.enc_fields,
                ":guid": &login.id,
                // time_last_used has been set to now.
                ":now_millis": login.time_last_used,
            },
        )?;
        Ok(())
    }

    pub fn import_multiple(
        &self,
        logins: Vec<Login>,
        encdec: &EncryptorDecryptor,
    ) -> Result<MigrationMetrics> {
        // Check if the logins table is empty first.
        let mut num_existing_logins =
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsL", NO_PARAMS, |r| r.get(0))?;
        num_existing_logins +=
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsM", NO_PARAMS, |r| r.get(0))?;
        if num_existing_logins > 0 {
            return Err(ErrorKind::NonEmptyTable.into());
        }
        let tx = self.unchecked_transaction()?;
        let import_start = Instant::now();
        let import_start_total_logins: u64 = logins.len() as u64;
        let mut num_failed_fixup: u64 = 0;
        let mut num_failed_insert: u64 = 0;
        let mut fixup_phase_duration = Duration::new(0, 0);
        let mut fixup_errors: Vec<String> = Vec::new();
        let mut insert_errors: Vec<String> = Vec::new();

        for login in logins.into_iter() {
            let old_guid = login.guid();
            let decrypted = login.decrypt_fields(encdec)?;
            let login = match self.fixup_and_check_for_dupes(
                &Guid::empty(),
                login.fields,
                decrypted,
                encdec,
            ) {
                Ok((new_fields, new_enc)) => Login {
                    id: if old_guid.is_valid_for_sync_server() {
                        old_guid.to_string()
                    } else {
                        Guid::random().to_string()
                    },
                    fields: new_fields,
                    enc_fields: new_enc.encrypt(encdec)?,
                    ..login
                },
                Err(e) => {
                    log::warn!("Skipping login {} as it is invalid ({}).", old_guid, e);
                    fixup_errors.push(e.label().into());
                    num_failed_fixup += 1;
                    continue;
                }
            };
            // Now we can safely insert it, knowing that it's valid data.
            fixup_phase_duration = import_start.elapsed();
            match self.insert_new_login(&login) {
                Ok(_) => log::info!(
                    "Imported {} (new GUID {}) successfully.",
                    old_guid,
                    login.id
                ),
                Err(e) => {
                    log::warn!("Could not import {} ({}).", old_guid, e);
                    insert_errors.push(e.label().into());
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
                total_duration: fixup_phase_duration.as_millis() as u64,
                errors: fixup_errors,
            },
            insert_phase: MigrationPhaseMetrics {
                num_processed: num_post_fixup,
                num_succeeded: num_post_fixup - num_failed_insert,
                num_failed: num_failed_insert,
                total_duration: insert_phase_duration.as_millis() as u64,
                errors: insert_errors,
            },
            num_processed: import_start_total_logins,
            num_succeeded: import_start_total_logins - num_failed,
            num_failed,
            total_duration: fixup_phase_duration
                .checked_add(insert_phase_duration)
                .unwrap_or_else(|| Duration::new(0, 0))
                .as_millis() as u64,
            errors: all_errors,
        };
        log::info!(
            "Finished importing logins with the following metrics: {:#?}",
            metrics
        );
        Ok(metrics)
    }

    pub fn add(&self, login: UpdatableLogin, encdec: &EncryptorDecryptor) -> Result<Login> {
        let guid = Guid::random();
        let now_ms = util::system_time_ms_i64(SystemTime::now());

        let (new_fields, new_enc) =
            self.fixup_and_check_for_dupes(&guid, login.fields, login.enc_fields, &encdec)?;
        let result = Login {
            id: guid.to_string(),
            fields: new_fields,
            enc_fields: new_enc.encrypt(&encdec)?,
            time_created: now_ms,
            time_password_changed: now_ms,
            time_last_used: now_ms,
            times_used: 1,
        };
        let tx = self.unchecked_transaction()?;
        self.insert_new_login(&result)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn update(
        &self,
        sguid: &str,
        login: UpdatableLogin,
        encdec: &EncryptorDecryptor,
    ) -> Result<Login> {
        let guid = Guid::new(sguid);
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        let tx = self.unchecked_transaction()?;

        // XXX - it's not clear that throwing here on a dupe is the correct thing to do - eg, a
        // user updated the username to one that already exists - the better thing to do is
        // probably just remove the dupe.
        let (new_fields, new_enc_fields) =
            self.fixup_and_check_for_dupes(&guid, login.fields, login.enc_fields, &encdec)?;
        let login = UpdatableLogin {
            fields: new_fields,
            enc_fields: new_enc_fields,
        };

        // Note: These fail with DuplicateGuid if the record doesn't exist.
        self.ensure_local_overlay_exists(&guid)?;
        self.mark_mirror_overridden(&guid)?;

        // We must read the existing record so we can correctly manage timePasswordChanged.
        let existing = match self.get_by_id(&sguid)? {
            Some(e) => e,
            None => throw!(ErrorKind::NoSuchRecord(sguid.to_owned())),
        };
        let time_password_changed =
            if existing.decrypt_fields(encdec)?.password == login.enc_fields.password {
                existing.time_password_changed
            } else {
                now_ms
            };

        // Make the final object here - every column will be updated.
        let result = Login {
            id: existing.id,
            fields: login.fields,
            enc_fields: login.enc_fields.encrypt(&encdec)?,
            time_created: existing.time_created,
            time_password_changed,
            time_last_used: now_ms,
            times_used: existing.times_used + 1,
        };

        self.update_existing_login(&result)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn check_valid_with_no_dupes(
        &self,
        guid: &Guid,
        fields: &LoginFields,
        enc_fields: &EncryptedFields,
        encdec: &EncryptorDecryptor,
    ) -> Result<()> {
        fields.check_valid()?;
        self.check_for_dupes(guid, fields, enc_fields, encdec)
    }

    pub fn fixup_and_check_for_dupes(
        &self,
        guid: &Guid,
        fields: LoginFields,
        enc_fields: EncryptedFields,
        encdec: &EncryptorDecryptor,
    ) -> Result<(LoginFields, EncryptedFields)> {
        let fields = fields.fixup()?;
        let enc_fields = enc_fields.fixup()?;
        self.check_for_dupes(guid, &fields, &enc_fields, encdec)?;
        Ok((fields, enc_fields))
    }

    pub fn check_for_dupes(
        &self,
        guid: &Guid,
        fields: &LoginFields,
        enc_fields: &EncryptedFields,
        encdec: &EncryptorDecryptor,
    ) -> Result<()> {
        if self.dupe_exists(guid, fields, enc_fields, encdec)? {
            throw!(InvalidLogin::DuplicateLogin);
        }
        Ok(())
    }

    pub fn dupe_exists(
        &self,
        guid: &Guid,
        fields: &LoginFields,
        enc_fields: &EncryptedFields,
        encdec: &EncryptorDecryptor,
    ) -> Result<bool> {
        Ok(self.find_dupe(guid, fields, enc_fields, encdec)?.is_some())
    }

    pub fn find_dupe(
        &self,
        guid: &Guid,
        fields: &LoginFields,
        enc_fields: &EncryptedFields,
        encdec: &EncryptorDecryptor,
    ) -> Result<Option<Guid>> {
        for possible in self.potential_dupes_ignoring_username(guid, fields)? {
            let pos_enc_fields = possible.decrypt_fields(encdec)?;
            if pos_enc_fields.username == enc_fields.username {
                return Ok(Some(possible.guid()));
            }
        }
        Ok(None)
    }

    pub fn potential_dupes_ignoring_username(
        &self,
        guid: &Guid,
        fields: &LoginFields,
    ) -> Result<Vec<Login>> {
        // Could be lazy_static-ed...
        lazy_static::lazy_static! {
            static ref DUPES_IGNORING_USERNAME_SQL: String = format!(
                "SELECT {common_cols} FROM loginsL
                WHERE is_deleted = 0
                    AND guid <> :guid
                    AND origin = :origin
                    AND (
                        formActionOrigin = :form_submit
                        OR
                        httpRealm = :http_realm
                    )

                UNION ALL

                SELECT {common_cols} FROM loginsM
                WHERE is_overridden = 0
                AND guid <> :guid
                AND origin = :origin
                    AND (
                        formActionOrigin = :form_submit
                        OR
                        httpRealm = :http_realm
                    )
                ",
                common_cols = schema::COMMON_COLS
            );
        }
        let mut stmt = self.db.prepare_cached(&DUPES_IGNORING_USERNAME_SQL)?;
        let params = named_params! {
            ":guid": guid,
            ":origin": &fields.origin,
            ":http_realm": fields.http_realm.as_ref(),
            ":form_submit": fields.form_action_origin.as_ref(),
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
                     encFields = '',
                     origin = ''
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
                    (guid, local_modified, is_deleted, sync_status, origin, timeCreated, timePasswordChanged, encFields)
            SELECT   guid, :now_ms,        1,          {changed},   '',     timeCreated, :now_ms,             ''
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
                    encFields = '',
                    origin = ''
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
                      (guid, local_modified, is_deleted, sync_status, origin, timeCreated, timePasswordChanged, encFields)
                SELECT guid, :now_ms,        1,          {changed},   '',     timeCreated, :now_ms,             ''
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
         ORDER BY origin ASC

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
    use crate::encryption::test_utils::decrypt_struct;
    use crate::login::test_utils::login;
    use crate::EncryptedFields;
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
            db.insert_new_login(&login(guid, password)).unwrap();
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
                formActionOrigin,
                usernameField,
                passwordField,
                encFields,
                origin,

                timesUsed,
                timeLastUsed,
                timePasswordChanged,
                timeCreated,

                guid
            ) VALUES (
                :is_overridden,
                :server_modified,

                :http_realm,
                :form_action_origin,
                :username_field,
                :password_field,
                :enc_fields,
                :origin,

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
            ":http_realm": login.fields.http_realm,
            ":form_action_origin": login.fields.form_action_origin,
            ":username_field": login.fields.username_field,
            ":password_field": login.fields.password_field,
            ":origin": login.fields.origin,
            ":enc_fields": login.enc_fields,
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
                "SELECT encFields, local_modified, is_deleted FROM loginsL WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let enc: EncryptedFields = decrypt_struct(row.0);
        assert_eq!(enc.password, password);
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
                "SELECT encFields, server_modified, is_overridden FROM loginsM WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let enc: EncryptedFields = decrypt_struct(row.0);
        assert_eq!(enc.password, password);
        assert_eq!(row.1, server_modified);
        assert_eq!(row.2, is_overridden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_utils::TEST_ENCRYPTOR;
    use crate::EncryptedFields;

    #[test]
    fn test_check_valid_with_no_dupes() {
        let db = LoginDb::open_in_memory().unwrap();
        let added = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        form_action_origin: Some("https://www.example.com".into()),
                        origin: "https://www.example.com".into(),
                        http_realm: None,
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "test".into(),
                        password: "test".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        let unique_login = UpdatableLogin {
            fields: LoginFields {
                form_action_origin: None,
                origin: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
        };

        let duplicate_login = UpdatableLogin {
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test2".into(),
            },
        };

        let updated_login = UpdatableLogin {
            fields: LoginFields {
                form_action_origin: None,
                origin: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test4".into(),
            },
        };

        struct TestCase {
            guid: Guid,
            login: UpdatableLogin,
            should_err: bool,
            expected_err: &'static str,
        }

        let test_cases = [
            TestCase {
                guid: "unique_value".into(),
                // unique_login should not error because it does not share the same origin,
                // username, and formActionOrigin or httpRealm with the pre-existing login
                // (login with guid `added.id`).
                login: unique_login,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                guid: "unique_value".into(),
                // duplicate_login has the same origin, username, and formActionOrigin as a pre-existing
                // login (guid `added.id`) and duplicate_login has no guid value, i.e. its guid
                // doesn't match with that of a pre-existing record so it can't be considered update,
                // so it should error.
                login: duplicate_login,
                should_err: true,
                expected_err: "Invalid login: Login already exists",
            },
            TestCase {
                // updated_login is an update to the existing record (has the same guid) so it is not a dupe
                // and should not error.
                guid: added.id.into(),
                login: updated_login,
                should_err: false,
                expected_err: "",
            },
        ];

        for tc in &test_cases {
            let login_check = db.check_valid_with_no_dupes(
                &tc.guid,
                &tc.login.fields,
                &tc.login.enc_fields,
                &TEST_ENCRYPTOR,
            );
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
        let added = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        form_action_origin: Some("http://😍.com".into()),
                        origin: "http://😍.com".into(),
                        http_realm: None,
                        username_field: "😍".into(),
                        password_field: "😍".into(),
                    },
                    enc_fields: EncryptedFields {
                        username: "😍".into(),
                        password: "😍".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        let fetched = db
            .get_by_id(&added.id)
            .expect("should work")
            .expect("should get a record");
        assert_eq!(added, fetched);
        assert_eq!(fetched.fields.origin, "http://xn--r28h.com");
        assert_eq!(
            fetched.fields.form_action_origin,
            Some("http://xn--r28h.com".to_string())
        );
        assert_eq!(fetched.fields.username_field, "😍");
        assert_eq!(fetched.fields.password_field, "😍");
        let enc_fields = fetched.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(enc_fields.username, "😍");
        assert_eq!(enc_fields.password, "😍");
    }

    #[test]
    fn test_unicode_realm() {
        let db = LoginDb::open_in_memory().unwrap();
        let added = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        form_action_origin: None,
                        origin: "http://😍.com".into(),
                        http_realm: Some("😍😍".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "😍".into(),
                        password: "😍".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        let fetched = db
            .get_by_id(&added.id)
            .expect("should work")
            .expect("should get a record");
        assert_eq!(added, fetched);
        assert_eq!(fetched.fields.origin, "http://xn--r28h.com");
        assert_eq!(fetched.fields.http_realm.unwrap(), "😍😍");
    }

    fn check_matches(db: &LoginDb, query: &str, expected: &[&str]) {
        let mut results = db
            .get_by_base_domain(query)
            .unwrap()
            .into_iter()
            .map(|l| l.fields.origin)
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
            db.add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: (*h).into(),
                        http_realm: Some((*h).into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        password: "test".into(),
                        ..Default::default()
                    },
                },
                &TEST_ENCRYPTOR,
            )
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
        let to_add = UpdatableLogin {
            fields: LoginFields {
                origin: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test_user".into(),
                password: "test_password".into(),
            },
        };
        let login = db.add(to_add, &TEST_ENCRYPTOR).unwrap();
        let login2 = db.get_by_id(&login.id).unwrap().unwrap();

        assert_eq!(login.fields.origin, login2.fields.origin);
        assert_eq!(login.fields.http_realm, login2.fields.http_realm);
        assert_eq!(login.enc_fields, login2.enc_fields);
    }

    #[test]
    fn test_update() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "user1".into(),
                        password: "password1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        db.update(
            &login.id,
            UpdatableLogin {
                fields: LoginFields {
                    origin: "https://www.example2.com".into(),
                    http_realm: Some("https://www.example2.com".into()),
                    ..login.fields
                },
                enc_fields: EncryptedFields {
                    username: "user2".into(),
                    password: "password2".into(),
                },
            },
            &TEST_ENCRYPTOR,
        )
        .unwrap();

        let login2 = db.get_by_id(&login.id).unwrap().unwrap();

        assert_eq!(login2.fields.origin, "https://www.example2.com");
        assert_eq!(
            login2.fields.http_realm,
            Some("https://www.example2.com".into())
        );
        let enc_fields = login2.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(enc_fields.username, "user2");
        assert_eq!(enc_fields.password, "password2");
    }

    #[test]
    fn test_touch() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "user1".into(),
                        password: "password1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        db.touch(&login.id).unwrap();
        let login2 = db.get_by_id(&login.id).unwrap().unwrap();
        assert!(login2.time_last_used > login.time_last_used);
        assert_eq!(login2.times_used, login.times_used + 1);
    }

    #[test]
    fn test_delete() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "test_user".into(),
                        password: "test_password".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        assert!(db.delete(login.guid_str()).unwrap());

        let tombstone_exists: bool = db
            .query_row_named(
                "SELECT EXISTS(
                    SELECT 1 FROM loginsL
                    WHERE guid = :guid AND is_deleted = 1
                )",
                named_params! { ":guid": login.guid_str() },
                |row| row.get(0),
            )
            .unwrap();

        assert!(tombstone_exists);
        assert!(!db.exists(login.guid_str()).unwrap());
    }

    #[test]
    fn test_wipe() {
        let db = LoginDb::open_in_memory().unwrap();
        let login1 = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "test_user_1".into(),
                        password: "test_password_1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        let login2 = db
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example2.com".into(),
                        http_realm: Some("https://www.example2.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "test_user_1".into(),
                        password: "test_password_2".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        db.wipe(&db.begin_interrupt_scope())
            .expect("wipe should work");

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
            .add(
                UpdatableLogin {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    enc_fields: EncryptedFields {
                        username: "test_user_1".into(),
                        password: "test_password_1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        let import_with_populated_table = db.import_multiple(Vec::new(), &TEST_ENCRYPTOR);
        assert!(import_with_populated_table.is_err());
        assert_eq!(
            import_with_populated_table.unwrap_err().to_string(),
            "The logins tables are not empty"
        );

        // Removing added login so the test cases below don't fail
        delete_logins(&db, &[login.id]).unwrap();

        // Setting up test cases
        let valid_login_guid1: Guid = Guid::random();
        let valid_login1 = Login {
            id: valid_login_guid1.to_string(),
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
            ..Default::default()
        };
        let valid_login_guid2: Guid = Guid::random();
        let valid_login2 = Login {
            id: valid_login_guid2.to_string(),
            fields: LoginFields {
                form_action_origin: Some("https://www.example2.com".into()),
                origin: "https://www.example2.com".into(),
                http_realm: None,
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test2".into(),
                password: "test2".into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
            ..Default::default()
        };
        let valid_login_guid3: Guid = Guid::random();
        let valid_login3 = Login {
            id: valid_login_guid3.to_string(),
            fields: LoginFields {
                form_action_origin: Some("https://www.example3.com".into()),
                origin: "https://www.example3.com".into(),
                http_realm: None,
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test3".into(),
                password: "test3".into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
            ..Default::default()
        };
        let duplicate_login_guid: Guid = Guid::random();
        let duplicate_login = Login {
            id: duplicate_login_guid.to_string(),
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test2".into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
            ..Default::default()
        };

        let duplicate_logins = vec![valid_login1.clone(), duplicate_login, valid_login2.clone()];

        let duplicate_logins_metrics = MigrationMetrics {
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

        let test_cases = vec![
            TestCase {
                logins: Vec::new(),
                has_populated_metrics: false,
                expected_metrics: MigrationMetrics {
                    ..MigrationMetrics::default()
                },
            },
            TestCase {
                logins: duplicate_logins,
                has_populated_metrics: true,
                expected_metrics: duplicate_logins_metrics,
            },
            TestCase {
                logins: valid_logins,
                has_populated_metrics: true,
                expected_metrics: valid_logins_metrics,
            },
        ];

        for tc in test_cases.into_iter() {
            let mut guids = Vec::new();
            for login in &tc.logins {
                guids.push(login.guid().into_string());
            }
            let import_result = db.import_multiple(tc.logins, &TEST_ENCRYPTOR);
            assert!(import_result.is_ok());

            let mut actual_metrics = import_result.unwrap();

            if tc.has_populated_metrics {
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

    #[test]
    fn test_import_multiple_bad_guid() {
        let db = LoginDb::open_in_memory().unwrap();
        let bad_guid = Guid::new("😍");
        assert!(!bad_guid.is_valid_for_sync_server());
        let login = Login {
            id: bad_guid.to_string(),
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                ..Default::default()
            },
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test2".into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
            ..Default::default()
        };
        db.import_multiple(vec![login], &TEST_ENCRYPTOR).unwrap();
        let logins = db.get_by_base_domain("www.example.com").unwrap();
        assert_eq!(logins.len(), 1);
        assert_ne!(logins[0].id, bad_guid, "guid was fixed");
    }
}
