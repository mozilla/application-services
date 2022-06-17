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
use crate::login::*;
use crate::schema;
use crate::sync::SyncStatus;
use crate::util;
use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
use lazy_static::lazy_static;
use rusqlite::{
    named_params,
    types::{FromSql, ToSql},
    Connection,
};
use sql_support::ConnExt;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use sync_guid::Guid;
use url::{Host, Url};

pub struct LoginDb {
    pub db: Connection,
    interrupt_handle: Arc<SqlInterruptHandle>,
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
            interrupt_handle: Arc::new(SqlInterruptHandle::new(&db)),
            db,
        };
        let tx = logins.db.transaction()?;
        schema::init(&tx)?;
        tx.commit()?;
        Ok(logins)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    pub fn new_interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        Arc::clone(&self.interrupt_handle)
    }

    #[inline]
    pub fn begin_interrupt_scope(&self) -> Result<SqlInterruptScope> {
        Ok(self.interrupt_handle.begin_interrupt_scope()?)
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
        self.execute_cached(
            "REPLACE INTO loginsSyncMeta (key, value) VALUES (:key, :value)",
            named_params! { ":key": key, ":value": value },
        )?;
        Ok(())
    }

    pub(crate) fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        self.try_query_row(
            "SELECT value FROM loginsSyncMeta WHERE key = :key",
            named_params! { ":key": key },
            |row| Ok::<_, LoginsError>(row.get(0)?),
            true,
        )
    }

    pub(crate) fn delete_meta(&self, key: &str) -> Result<()> {
        self.execute_cached(
            "DELETE FROM loginsSyncMeta WHERE key = :key",
            named_params! { ":key": key },
        )?;
        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<EncryptedLogin>> {
        let mut stmt = self.db.prepare_cached(&GET_ALL_SQL)?;
        let rows = stmt.query_and_then([], EncryptedLogin::from_row)?;
        rows.collect::<Result<_>>()
    }

    pub fn get_by_base_domain(&self, base_domain: &str) -> Result<Vec<EncryptedLogin>> {
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
            .query_and_then([], EncryptedLogin::from_row)?
            .filter(|r| {
                let login = r
                    .as_ref()
                    .ok()
                    .and_then(|login| Url::parse(&login.fields.origin).ok());
                let this_host = login.as_ref().and_then(|url| url.host());
                match (&base_host, this_host) {
                    (Host::Domain(base), Some(Host::Domain(look))) => {
                        // a fairly long-winded way of saying
                        // `login.fields.origin == base_domain ||
                        //  login.fields.origin.ends_with('.' + base_domain);`
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

    pub fn get_by_id(&self, id: &str) -> Result<Option<EncryptedLogin>> {
        self.try_query_row(
            &GET_BY_GUID_SQL,
            &[(":guid", &id as &dyn ToSql)],
            EncryptedLogin::from_row,
            true,
        )
    }

    // Match a `LoginEntry` being saved to existing logins in the DB
    //
    // When a user is saving new login, there are several cases for how we want to save the data:
    //
    //  - Adding a new login: `None` will be returned
    //  - Updating an existing login: `Some(login)` will be returned and the username will match
    //    the one for look.
    //  - Filling in a blank username for an existing login: `Some(login)` will be returned
    //    with a blank username.
    //
    //  Returns an Err if the new login is not valid and could not be fixed up
    pub fn find_login_to_update(
        &self,
        look: LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<Option<Login>> {
        let look = look.fixup()?;
        let logins = self
            .get_by_entry_target(&look)?
            .into_iter()
            .map(|enc_login| enc_login.decrypt(encdec))
            .collect::<Result<Vec<Login>>>()?;
        Ok(logins
            // First, try to match the username
            .iter()
            .find(|login| login.sec_fields.username == look.sec_fields.username)
            // Fall back on a blank username
            .or_else(|| {
                logins
                    .iter()
                    .find(|login| login.sec_fields.username.is_empty())
            })
            // Clone the login to avoid ref issues when returning across the FFI
            .cloned())
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        let tx = self.unchecked_transaction()?;
        self.ensure_local_overlay_exists(id)?;
        self.mark_mirror_overridden(id)?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        // As on iOS, just using a record doesn't flip it's status to changed.
        // TODO: this might be wrong for lockbox!
        self.execute_cached(
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
    fn insert_new_login(&self, login: &EncryptedLogin) -> Result<()> {
        let sql = format!(
            "INSERT INTO loginsL (
                origin,
                httpRealm,
                formActionOrigin,
                usernameField,
                passwordField,
                timesUsed,
                secFields,
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
                :sec_fields,
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

        self.execute(
            &sql,
            named_params! {
                ":origin": login.fields.origin,
                ":http_realm": login.fields.http_realm,
                ":form_action_origin": login.fields.form_action_origin,
                ":username_field": login.fields.username_field,
                ":password_field": login.fields.password_field,
                ":time_created": login.record.time_created,
                ":times_used": login.record.times_used,
                ":time_last_used": login.record.time_last_used,
                ":time_password_changed": login.record.time_password_changed,
                ":local_modified": login.record.time_created,
                ":sec_fields": login.sec_fields,
                ":guid": login.guid(),
            },
        )?;
        Ok(())
    }

    fn update_existing_login(&self, login: &EncryptedLogin) -> Result<()> {
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
                 secFields           = :sec_fields,
                 origin              = :origin,
                 -- leave New records as they are, otherwise update them to `changed`
                 sync_status         = max(sync_status, {changed})
             WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );

        self.db.execute(
            &sql,
            named_params! {
                ":origin": login.fields.origin,
                ":http_realm": login.fields.http_realm,
                ":form_action_origin": login.fields.form_action_origin,
                ":username_field": login.fields.username_field,
                ":password_field": login.fields.password_field,
                ":time_last_used": login.record.time_last_used,
                ":times_used": login.record.times_used,
                ":time_password_changed": login.record.time_password_changed,
                ":sec_fields": login.sec_fields,
                ":guid": &login.record.id,
                // time_last_used has been set to now.
                ":now_millis": login.record.time_last_used,
            },
        )?;
        Ok(())
    }

    pub fn import_multiple(&self, logins: Vec<Login>, encdec: &EncryptorDecryptor) -> Result<()> {
        // Check if the logins table is empty first.
        let mut num_existing_logins =
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsL", [], |r| r.get(0))?;
        num_existing_logins +=
            self.query_row::<i64, _, _>("SELECT COUNT(*) FROM loginsM", [], |r| r.get(0))?;
        if num_existing_logins > 0 {
            return Err(LoginsError::NonEmptyTable);
        }
        let tx = self.unchecked_transaction()?;

        for login in logins.into_iter() {
            let old_guid = login.guid();
            let login = match self.fixup_and_check_for_dupes(&Guid::empty(), login.entry(), encdec)
            {
                Ok(new_entry) => EncryptedLogin {
                    record: RecordFields {
                        id: if old_guid.is_valid_for_sync_server() {
                            old_guid.to_string()
                        } else {
                            Guid::random().to_string()
                        },
                        ..login.record
                    },
                    fields: new_entry.fields,
                    sec_fields: new_entry.sec_fields.encrypt(encdec)?,
                },
                Err(e) => {
                    log::warn!("Skipping login {} as it is invalid ({}).", old_guid, e);
                    continue;
                }
            };
            // Now we can safely insert it, knowing that it's valid data.
            match self.insert_new_login(&login) {
                Ok(_) => log::info!(
                    "Imported {} (new GUID {}) successfully.",
                    old_guid,
                    login.record.id
                ),
                Err(e) => {
                    log::warn!("Could not import {} ({}).", old_guid, e);
                }
            };
        }
        tx.commit()?;

        Ok(())
    }

    pub fn add(&self, entry: LoginEntry, encdec: &EncryptorDecryptor) -> Result<EncryptedLogin> {
        let guid = Guid::random();
        let now_ms = util::system_time_ms_i64(SystemTime::now());

        let new_entry = self.fixup_and_check_for_dupes(&guid, entry, encdec)?;
        let result = EncryptedLogin {
            record: RecordFields {
                id: guid.to_string(),
                time_created: now_ms,
                time_password_changed: now_ms,
                time_last_used: now_ms,
                times_used: 1,
            },
            fields: new_entry.fields,
            sec_fields: new_entry.sec_fields.encrypt(encdec)?,
        };
        let tx = self.unchecked_transaction()?;
        self.insert_new_login(&result)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn update(
        &self,
        sguid: &str,
        entry: LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<EncryptedLogin> {
        let guid = Guid::new(sguid);
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        let tx = self.unchecked_transaction()?;

        let entry = entry.fixup()?;

        // Check if there's an existing login that's the dupe of this login.  That indicates that
        // something has gone wrong with our underlying logic.  However, if we do see a dupe login,
        // just log an error and continue.  This avoids a crash on android-components
        // (mozilla-mobile/android-components#11251).

        if self.check_for_dupes(&guid, &entry, encdec).is_err() {
            // Try to detect if sync is enabled by checking if there are any mirror logins
            let has_mirror_row: bool =
                self.db.query_one("SELECT EXISTS (SELECT 1 FROM loginsM)")?;
            log::error!("Duplicate in update() (has_mirror_row: {})", has_mirror_row);
        }

        // Note: This fail with NoSuchRecord if the record doesn't exist.
        self.ensure_local_overlay_exists(&guid)?;
        self.mark_mirror_overridden(&guid)?;

        // We must read the existing record so we can correctly manage timePasswordChanged.
        let existing = match self.get_by_id(sguid)? {
            Some(e) => e,
            None => return Err(LoginsError::NoSuchRecord(sguid.to_owned())),
        };
        let time_password_changed =
            if existing.decrypt_fields(encdec)?.password == entry.sec_fields.password {
                existing.record.time_password_changed
            } else {
                now_ms
            };

        // Make the final object here - every column will be updated.
        let result = EncryptedLogin {
            record: RecordFields {
                id: existing.record.id,
                time_created: existing.record.time_created,
                time_password_changed,
                time_last_used: now_ms,
                times_used: existing.record.times_used + 1,
            },
            fields: entry.fields,
            sec_fields: entry.sec_fields.encrypt(encdec)?,
        };

        self.update_existing_login(&result)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn add_or_update(
        &self,
        entry: LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<EncryptedLogin> {
        // Make sure to fixup the entry first, in case that changes the username
        let entry = entry.fixup()?;
        match self.find_login_to_update(entry.clone(), encdec)? {
            Some(login) => self.update(&login.record.id, entry, encdec),
            None => self.add(entry, encdec),
        }
    }

    pub fn fixup_and_check_for_dupes(
        &self,
        guid: &Guid,
        entry: LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<LoginEntry> {
        let entry = entry.fixup()?;
        self.check_for_dupes(guid, &entry, encdec)?;
        Ok(entry)
    }

    pub fn check_for_dupes(
        &self,
        guid: &Guid,
        entry: &LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<()> {
        if self.dupe_exists(guid, entry, encdec)? {
            return Err(InvalidLogin::DuplicateLogin.into());
        }
        Ok(())
    }

    pub fn dupe_exists(
        &self,
        guid: &Guid,
        entry: &LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<bool> {
        Ok(self.find_dupe(guid, entry, encdec)?.is_some())
    }

    pub fn find_dupe(
        &self,
        guid: &Guid,
        entry: &LoginEntry,
        encdec: &EncryptorDecryptor,
    ) -> Result<Option<Guid>> {
        for possible in self.get_by_entry_target(entry)? {
            if possible.guid() != *guid {
                let pos_sec_fields = possible.decrypt_fields(encdec)?;
                if pos_sec_fields.username == entry.sec_fields.username {
                    return Ok(Some(possible.guid()));
                }
            }
        }
        Ok(None)
    }

    // Find saved logins that match the target for a `LoginEntry`
    //
    // This means that:
    //   - `origin` matches
    //   - Either `form_action_origin` or `http_realm` matches, depending on which one is non-null
    //
    // This is used for dupe-checking and `find_login_to_update()`
    fn get_by_entry_target(&self, entry: &LoginEntry) -> Result<Vec<EncryptedLogin>> {
        // Could be lazy_static-ed...
        lazy_static::lazy_static! {
            static ref GET_BY_FORM_ACTION_ORIGIN: String = format!(
                "SELECT {common_cols} FROM loginsL
                WHERE is_deleted = 0
                    AND origin = :origin
                    AND formActionOrigin = :form_action_origin

                UNION ALL

                SELECT {common_cols} FROM loginsM
                WHERE is_overridden = 0
                    AND origin = :origin
                    AND formActionOrigin = :form_action_origin
                ",
                common_cols = schema::COMMON_COLS
            );
            static ref GET_BY_HTTP_REALM: String = format!(
                "SELECT {common_cols} FROM loginsL
                WHERE is_deleted = 0
                    AND origin = :origin
                    AND httpRealm = :http_realm

                UNION ALL

                SELECT {common_cols} FROM loginsM
                WHERE is_overridden = 0
                    AND origin = :origin
                    AND httpRealm = :http_realm
                ",
                common_cols = schema::COMMON_COLS
            );
        }
        match (
            entry.fields.form_action_origin.as_ref(),
            entry.fields.http_realm.as_ref(),
        ) {
            (Some(form_action_origin), None) => {
                let params = named_params! {
                    ":origin": &entry.fields.origin,
                    ":form_action_origin": form_action_origin,
                };
                self.db
                    .prepare_cached(&GET_BY_FORM_ACTION_ORIGIN)?
                    .query_and_then(params, EncryptedLogin::from_row)?
                    .collect()
            }
            (None, Some(http_realm)) => {
                let params = named_params! {
                    ":origin": &entry.fields.origin,
                    ":http_realm": http_realm,
                };
                self.db
                    .prepare_cached(&GET_BY_HTTP_REALM)?
                    .query_and_then(params, EncryptedLogin::from_row)?
                    .collect()
            }
            (Some(_), Some(_)) => Err(InvalidLogin::BothTargets.into()),
            (None, None) => Err(InvalidLogin::NoTarget.into()),
        }
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row(
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
        self.execute(
            &format!(
                "UPDATE loginsL
                 SET local_modified = :now_ms,
                     sync_status = {status_changed},
                     is_deleted = 1,
                     secFields = '',
                     origin = '', 
                     httpRealm = NULL,
                     formActionOrigin = NULL
                 WHERE guid = :guid",
                status_changed = SyncStatus::Changed as u8
            ),
            named_params! { ":now_ms": now_ms, ":guid": id },
        )?;

        // Mark the mirror as overridden
        self.execute(
            "UPDATE loginsM SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": id },
        )?;

        // If we don't have a local record for this ID, but do have it in the mirror
        // insert a tombstone.
        self.execute(&format!("
            INSERT OR IGNORE INTO loginsL
                    (guid, local_modified, is_deleted, sync_status, origin, timeCreated, timePasswordChanged, secFields)
            SELECT   guid, :now_ms,        1,          {changed},   '',     timeCreated, :now_ms,             ''
            FROM loginsM
            WHERE guid = :guid",
            changed = SyncStatus::Changed as u8),
            named_params! { ":now_ms": now_ms, ":guid": id })?;
        tx.commit()?;
        Ok(exists)
    }

    fn mark_mirror_overridden(&self, guid: &str) -> Result<()> {
        self.execute_cached(
            "UPDATE loginsM SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": guid },
        )?;
        Ok(())
    }

    fn ensure_local_overlay_exists(&self, guid: &str) -> Result<()> {
        let already_have_local: bool = self.db.query_row(
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
            return Err(LoginsError::NoSuchRecord(guid.to_owned()));
        }
        Ok(())
    }

    fn clone_mirror_to_overlay(&self, guid: &str) -> Result<usize> {
        Ok(self.execute_cached(&*CLONE_SINGLE_MIRROR_SQL, &[(":guid", &guid as &dyn ToSql)])?)
    }

    // Wipe is called both by Sync and also exposed publically, so it's
    // implemented here.
    pub(crate) fn wipe(&self, scope: &SqlInterruptScope) -> Result<()> {
        let tx = self.unchecked_transaction()?;
        log::info!("Executing wipe on password engine!");
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        scope.err_if_interrupted()?;
        self.execute(
            &format!(
                "
                UPDATE loginsL
                SET local_modified = :now_ms,
                    sync_status = {changed},
                    is_deleted = 1,
                    secFields = '',
                    origin = ''
                WHERE is_deleted = 0",
                changed = SyncStatus::Changed as u8
            ),
            named_params! { ":now_ms": now_ms },
        )?;
        scope.err_if_interrupted()?;

        self.execute("UPDATE loginsM SET is_overridden = 1", [])?;
        scope.err_if_interrupted()?;

        self.execute(
            &format!("
                INSERT OR IGNORE INTO loginsL
                      (guid, local_modified, is_deleted, sync_status, origin, timeCreated, timePasswordChanged, secFields)
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
    use crate::login::test_utils::enc_login;
    use crate::SecureLoginFields;
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
                db,
                &enc_login(guid, password),
                &ServerTimestamp(util::system_time_ms_i64(std::time::SystemTime::now())),
                local_login.is_some(),
            )
            .unwrap();
        }
        if let Some(password) = local_login {
            db.insert_new_login(&enc_login(guid, password)).unwrap();
        }
    }

    pub fn add_mirror(
        db: &LoginDb,
        login: &EncryptedLogin,
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
                secFields,
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
                :sec_fields,
                :origin,

                :times_used,
                :time_last_used,
                :time_password_changed,
                :time_created,

                :guid
            )";
        let mut stmt = db.prepare_cached(sql)?;

        stmt.execute(named_params! {
            ":is_overridden": is_overridden,
            ":server_modified": server_modified.as_millis(),
            ":http_realm": login.fields.http_realm,
            ":form_action_origin": login.fields.form_action_origin,
            ":username_field": login.fields.username_field,
            ":password_field": login.fields.password_field,
            ":origin": login.fields.origin,
            ":sec_fields": login.sec_fields,
            ":times_used": login.record.times_used,
            ":time_last_used": login.record.time_last_used,
            ":time_password_changed": login.record.time_password_changed,
            ":time_created": login.record.time_created,
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
            .query_map([], |r| r.get(0))
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
                "SELECT secFields, local_modified, is_deleted FROM loginsL WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let enc: SecureLoginFields = decrypt_struct(row.0);
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
                "SELECT secFields, server_modified, is_overridden FROM loginsM WHERE guid=?",
                &[guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let enc: SecureLoginFields = decrypt_struct(row.0);
        assert_eq!(enc.password, password);
        assert_eq!(row.1, server_modified);
        assert_eq!(row.2, is_overridden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::test_utils::TEST_ENCRYPTOR;
    use crate::sync::LocalLogin;
    use crate::SecureLoginFields;

    #[test]
    fn test_username_dupe_semantics() {
        let mut login = LoginEntry {
            fields: LoginFields {
                origin: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                ..LoginFields::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "sekret".into(),
            },
        };

        let db = LoginDb::open_in_memory().unwrap();
        db.add(login.clone(), &TEST_ENCRYPTOR)
            .expect("should be able to add first login");

        // We will reject new logins with the same username value...
        let exp_err = "Invalid login: Login already exists";
        assert_eq!(
            db.add(login.clone(), &TEST_ENCRYPTOR)
                .unwrap_err()
                .to_string(),
            exp_err
        );

        // Add one with an empty username - not a dupe.
        login.sec_fields.username = "".to_string();
        db.add(login.clone(), &TEST_ENCRYPTOR)
            .expect("empty login isn't a dupe");

        assert_eq!(
            db.add(login, &TEST_ENCRYPTOR).unwrap_err().to_string(),
            exp_err
        );

        // one with a username, 1 without.
        assert_eq!(db.get_all().unwrap().len(), 2);
    }

    #[test]
    fn test_unicode_submit() {
        let db = LoginDb::open_in_memory().unwrap();
        let added = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        form_action_origin: Some("http://😍.com".into()),
                        origin: "http://😍.com".into(),
                        http_realm: None,
                        username_field: "😍".into(),
                        password_field: "😍".into(),
                    },
                    sec_fields: SecureLoginFields {
                        username: "😍".into(),
                        password: "😍".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        let fetched = db
            .get_by_id(&added.record.id)
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
        let sec_fields = fetched.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(sec_fields.username, "😍");
        assert_eq!(sec_fields.password, "😍");
    }

    #[test]
    fn test_unicode_realm() {
        let db = LoginDb::open_in_memory().unwrap();
        let added = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        form_action_origin: None,
                        origin: "http://😍.com".into(),
                        http_realm: Some("😍😍".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "😍".into(),
                        password: "😍".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        let fetched = db
            .get_by_id(&added.record.id)
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
                LoginEntry {
                    fields: LoginFields {
                        origin: (*h).into(),
                        http_realm: Some((*h).into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
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
        let to_add = LoginEntry {
            fields: LoginFields {
                origin: "https://www.example.com".into(),
                http_realm: Some("https://www.example.com".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test_user".into(),
                password: "test_password".into(),
            },
        };
        let login = db.add(to_add, &TEST_ENCRYPTOR).unwrap();
        let login2 = db.get_by_id(&login.record.id).unwrap().unwrap();

        assert_eq!(login.fields.origin, login2.fields.origin);
        assert_eq!(login.fields.http_realm, login2.fields.http_realm);
        assert_eq!(login.sec_fields, login2.sec_fields);
    }

    #[test]
    fn test_update() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "user1".into(),
                        password: "password1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        db.update(
            &login.record.id,
            LoginEntry {
                fields: LoginFields {
                    origin: "https://www.example2.com".into(),
                    http_realm: Some("https://www.example2.com".into()),
                    ..login.fields
                },
                sec_fields: SecureLoginFields {
                    username: "user2".into(),
                    password: "password2".into(),
                },
            },
            &TEST_ENCRYPTOR,
        )
        .unwrap();

        let login2 = db.get_by_id(&login.record.id).unwrap().unwrap();

        assert_eq!(login2.fields.origin, "https://www.example2.com");
        assert_eq!(
            login2.fields.http_realm,
            Some("https://www.example2.com".into())
        );
        let sec_fields = login2.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(sec_fields.username, "user2");
        assert_eq!(sec_fields.password, "password2");
    }

    #[test]
    fn test_touch() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "user1".into(),
                        password: "password1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
        db.touch(&login.record.id).unwrap();
        let login2 = db.get_by_id(&login.record.id).unwrap().unwrap();
        assert!(login2.record.time_last_used > login.record.time_last_used);
        assert_eq!(login2.record.times_used, login.record.times_used + 1);
    }

    #[test]
    fn test_delete() {
        let db = LoginDb::open_in_memory().unwrap();
        let login = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "test_user".into(),
                        password: "test_password".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        assert!(db.delete(login.guid_str()).unwrap());

        let local_login = db
            .query_row(
                "SELECT * FROM loginsL WHERE guid = :guid",
                named_params! { ":guid": login.guid_str() },
                |row| Ok(LocalLogin::from_row(row).unwrap()),
            )
            .unwrap();
        assert!(local_login.is_deleted);
        assert_eq!(local_login.login.fields.http_realm, None);
        assert_eq!(local_login.login.fields.form_action_origin, None);

        assert!(!db.exists(login.guid_str()).unwrap());
    }

    #[test]
    fn test_wipe() {
        let db = LoginDb::open_in_memory().unwrap();
        let login1 = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "test_user_1".into(),
                        password: "test_password_1".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        let login2 = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example2.com".into(),
                        http_realm: Some("https://www.example2.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "test_user_1".into(),
                        password: "test_password_2".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();

        db.wipe(&db.begin_interrupt_scope().unwrap())
            .expect("wipe should work");

        let expected_tombstone_count = 2;
        let actual_tombstone_count: i32 = db
            .query_row(
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
                rusqlite::params_from_iter(chunk),
            )?;
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn test_import_multiple() {
        let db = LoginDb::open_in_memory().unwrap();

        // Adding login to trigger non-empty table error
        let login = db
            .add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("https://www.example.com".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
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
        delete_logins(&db, &[login.record.id]).unwrap();

        // Setting up test cases
        let valid_login_guid1: Guid = Guid::random();
        let valid_login1 = Login {
            record: RecordFields {
                id: valid_login_guid1.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "test".into(),
            },
        };
        let valid_login_guid2: Guid = Guid::random();
        let valid_login2 = Login {
            record: RecordFields {
                id: valid_login_guid2.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example2.com".into()),
                origin: "https://www.example2.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test2".into(),
                password: "test2".into(),
            },
        };
        let valid_login_guid3: Guid = Guid::random();
        let valid_login3 = Login {
            record: RecordFields {
                id: valid_login_guid3.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example3.com".into()),
                origin: "https://www.example3.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test3".into(),
                password: "test3".into(),
            },
        };
        let duplicate_login_guid: Guid = Guid::random();
        let duplicate_login = Login {
            record: RecordFields {
                id: duplicate_login_guid.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                http_realm: None,
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "test2".into(),
            },
        };

        let duplicate_logins = vec![valid_login1.clone(), duplicate_login, valid_login2.clone()];

        let valid_logins = vec![valid_login1, valid_login2, valid_login3];

        let test_cases = vec![Vec::new(), duplicate_logins, valid_logins];

        for logins in test_cases.into_iter() {
            let mut guids = Vec::new();
            for login in &logins {
                guids.push(login.guid().into_string());
            }
            db.import_multiple(logins, &TEST_ENCRYPTOR)
                .expect("import should work");
            // clearing the database for next test case
            delete_logins(&db, guids.as_slice()).unwrap();
        }
    }

    #[test]
    fn test_import_multiple_bad_guid() {
        let db = LoginDb::open_in_memory().unwrap();
        let bad_guid = Guid::new("😍");
        assert!(!bad_guid.is_valid_for_sync_server());
        let login = Login {
            record: RecordFields {
                id: bad_guid.to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "https://www.example.com".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "test2".into(),
            },
        };
        db.import_multiple(vec![login], &TEST_ENCRYPTOR).unwrap();
        let logins = db.get_by_base_domain("www.example.com").unwrap();
        assert_eq!(logins.len(), 1);
        assert_ne!(logins[0].record.id, bad_guid, "guid was fixed");
    }

    mod test_find_login_to_update {
        use super::*;

        fn make_entry(username: &str, password: &str) -> LoginEntry {
            LoginEntry {
                fields: LoginFields {
                    origin: "https://www.example.com".into(),
                    http_realm: Some("the website".into()),
                    ..Default::default()
                },
                sec_fields: SecureLoginFields {
                    username: username.into(),
                    password: password.into(),
                },
            }
        }

        fn make_saved_login(db: &LoginDb, username: &str, password: &str) -> Login {
            db.add(make_entry(username, password), &TEST_ENCRYPTOR)
                .unwrap()
                .decrypt(&TEST_ENCRYPTOR)
                .unwrap()
        }

        #[test]
        fn test_match() {
            let db = LoginDb::open_in_memory().unwrap();
            let login = make_saved_login(&db, "user", "pass");
            assert_eq!(
                Some(login),
                db.find_login_to_update(make_entry("user", "pass"), &TEST_ENCRYPTOR)
                    .unwrap(),
            );
        }

        #[test]
        fn test_non_matches() {
            let db = LoginDb::open_in_memory().unwrap();
            // Non-match because the username is different
            make_saved_login(&db, "other-user", "pass");
            // Non-match because the http_realm is different
            db.add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        http_realm: Some("the other website".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "user".into(),
                        password: "pass".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
            // Non-match because it uses form_action_origin instead of http_realm
            db.add(
                LoginEntry {
                    fields: LoginFields {
                        origin: "https://www.example.com".into(),
                        form_action_origin: Some("https://www.example.com/".into()),
                        ..Default::default()
                    },
                    sec_fields: SecureLoginFields {
                        username: "user".into(),
                        password: "pass".into(),
                    },
                },
                &TEST_ENCRYPTOR,
            )
            .unwrap();
            assert_eq!(
                None,
                db.find_login_to_update(make_entry("user", "pass"), &TEST_ENCRYPTOR)
                    .unwrap(),
            );
        }

        #[test]
        fn test_match_blank_password() {
            let db = LoginDb::open_in_memory().unwrap();
            let login = make_saved_login(&db, "", "pass");
            assert_eq!(
                Some(login),
                db.find_login_to_update(make_entry("user", "pass"), &TEST_ENCRYPTOR)
                    .unwrap(),
            );
        }

        #[test]
        fn test_username_match_takes_precedence_over_blank_username() {
            let db = LoginDb::open_in_memory().unwrap();
            make_saved_login(&db, "", "pass");
            let username_match = make_saved_login(&db, "user", "pass");
            assert_eq!(
                Some(username_match),
                db.find_login_to_update(make_entry("user", "pass"), &TEST_ENCRYPTOR)
                    .unwrap(),
            );
        }

        #[test]
        fn test_invalid_login() {
            let db = LoginDb::open_in_memory().unwrap();
            assert!(db
                .find_login_to_update(
                    LoginEntry {
                        fields: LoginFields {
                            http_realm: None,
                            form_action_origin: None,
                            ..LoginFields::default()
                        },
                        ..LoginEntry::default()
                    },
                    &TEST_ENCRYPTOR
                )
                .is_err());
        }

        #[test]
        fn test_update_with_duplicate_login() {
            // If we have duplicate logins in the database, it should be possible to update them
            // without triggering a DuplicateLogin error
            let db = LoginDb::open_in_memory().unwrap();
            let login = make_saved_login(&db, "user", "pass");
            let mut dupe = login.clone().encrypt(&TEST_ENCRYPTOR).unwrap();
            dupe.record.id = "different-guid".to_string();
            db.insert_new_login(&dupe).unwrap();

            let mut entry = login.entry();
            entry.sec_fields.password = "pass2".to_string();
            db.update(&login.record.id, entry, &TEST_ENCRYPTOR).unwrap();

            let mut entry = login.entry();
            entry.sec_fields.password = "pass3".to_string();
            db.add_or_update(entry, &TEST_ENCRYPTOR).unwrap();
        }
    }
}
