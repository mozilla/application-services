/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::merge::{LocalLogin, MirrorLogin, SyncLoginData};
use super::update_plan::UpdatePlan;
use super::SyncStatus;
use crate::db::CLONE_ENTIRE_MIRROR_SQL;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::login::EncryptedLogin;
use crate::schema;
use crate::util;
use crate::LoginDb;
use crate::LoginStore;
use interrupt_support::SqlInterruptScope;
use rusqlite::named_params;
use sql_support::ConnExt;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;
use sync15::bso::{IncomingBso, OutgoingBso, OutgoingEnvelope};
use sync15::engine::{CollSyncIds, CollectionRequest, EngineSyncAssociation, SyncEngine};
use sync15::{telemetry, ServerTimestamp};
use sync_guid::Guid;

// The sync engine.
pub struct LoginsSyncEngine {
    pub store: Arc<LoginStore>,
    pub scope: SqlInterruptScope,
    pub staged: RefCell<Vec<IncomingBso>>,
    // It's unfortunate this is an Option<>, but tricky to change because sometimes we construct
    // an engine for, say, a `reset()` where this isn't needed or known.
    encdec: Option<EncryptorDecryptor>,
}

impl LoginsSyncEngine {
    fn encdec(&self) -> Result<&EncryptorDecryptor> {
        match &self.encdec {
            Some(encdec) => Ok(encdec),
            None => Err(Error::EncryptionKeyMissing),
        }
    }

    pub fn new(store: Arc<LoginStore>) -> Result<Self> {
        let scope = store.db.lock().begin_interrupt_scope()?;
        Ok(Self {
            store,
            scope,
            staged: RefCell::new(vec![]),
            encdec: None,
        })
    }

    fn reconcile(
        &self,
        records: Vec<SyncLoginData>,
        server_now: ServerTimestamp,
        telem: &mut telemetry::EngineIncoming,
    ) -> Result<UpdatePlan> {
        let mut plan = UpdatePlan::default();
        let encdec = self.encdec()?;

        for mut record in records {
            self.scope.err_if_interrupted()?;
            log::debug!("Processing remote change {}", record.guid());
            let upstream = if let Some(inbound) = record.inbound.take() {
                inbound
            } else {
                log::debug!("Processing inbound deletion (always prefer)");
                plan.plan_delete(record.guid.clone());
                continue;
            };
            let upstream_time = record.inbound_ts;
            match (record.mirror.take(), record.local.take()) {
                (Some(mirror), Some(local)) => {
                    log::debug!("  Conflict between remote and local, Resolving with 3WM");
                    plan.plan_three_way_merge(
                        local,
                        mirror,
                        upstream,
                        upstream_time,
                        server_now,
                        encdec,
                    )?;
                    telem.reconciled(1);
                }
                (Some(_mirror), None) => {
                    log::debug!("  Forwarding mirror to remote");
                    plan.plan_mirror_update(upstream, upstream_time);
                    telem.applied(1);
                }
                (None, Some(local)) => {
                    log::debug!("  Conflicting record without shared parent, using newer");
                    plan.plan_two_way_merge(&local.login, (upstream, upstream_time));
                    telem.reconciled(1);
                }
                (None, None) => {
                    if let Some(dupe) = self.find_dupe_login(&upstream.login)? {
                        log::debug!(
                            "  Incoming record {} was is a dupe of local record {}",
                            upstream.guid(),
                            dupe.guid()
                        );
                        plan.plan_two_way_merge(&dupe, (upstream, upstream_time));
                    } else {
                        log::debug!("  No dupe found, inserting into mirror");
                        plan.plan_mirror_insert(upstream, upstream_time, false);
                    }
                    telem.applied(1);
                }
            }
        }
        Ok(plan)
    }

    fn execute_plan(&self, plan: UpdatePlan) -> Result<()> {
        // Because rusqlite want a mutable reference to create a transaction
        // (as a way to save us from ourselves), we side-step that by creating
        // it manually.
        let db = self.store.db.lock();
        let tx = db.unchecked_transaction()?;
        plan.execute(&tx, &self.scope)?;
        tx.commit()?;
        Ok(())
    }

    // Fetch all the data for the provided IDs.
    // TODO: Might be better taking a fn instead of returning all of it... But that func will likely
    // want to insert stuff while we're doing this so ugh.
    fn fetch_login_data(
        &self,
        records: Vec<IncomingBso>,
        telem: &mut telemetry::EngineIncoming,
    ) -> Result<Vec<SyncLoginData>> {
        let mut sync_data = Vec::with_capacity(records.len());
        {
            let mut seen_ids: HashSet<Guid> = HashSet::with_capacity(records.len());
            for incoming in records.into_iter() {
                let id = incoming.envelope.id.clone();
                match SyncLoginData::from_bso(incoming, self.encdec()?) {
                    Ok(v) => sync_data.push(v),
                    Err(e) => {
                        match e {
                            // This is a known error with Deskop logins (see #5233), just log it
                            // rather than reporting to sentry
                            Error::InvalidLogin(InvalidLogin::IllegalOrigin) => {
                                log::warn!("logins-deserialize-error: {e}");
                            }
                            // For all other errors, report them to Sentry
                            _ => {
                                report_error!(
                                    "logins-deserialize-error",
                                    "Failed to deserialize record {:?}: {e}",
                                    id
                                );
                            }
                        };
                        // Ideally we'd track new_failed, but it's unclear how
                        // much value it has.
                        telem.failed(1);
                    }
                }
                seen_ids.insert(id);
            }
        }
        self.scope.err_if_interrupted()?;

        sql_support::each_chunk(
            &sync_data
                .iter()
                .map(|s| s.guid.as_str().to_string())
                .collect::<Vec<String>>(),
            |chunk, offset| -> Result<()> {
                // pairs the bound parameter for the guid with an integer index.
                let values_with_idx = sql_support::repeat_display(chunk.len(), ",", |i, f| {
                    write!(f, "({},?)", i + offset)
                });
                let query = format!(
                    "WITH to_fetch(guid_idx, fetch_guid) AS (VALUES {vals})
                     SELECT
                         {common_cols},
                         is_overridden,
                         server_modified,
                         NULL as local_modified,
                         NULL as is_deleted,
                         NULL as sync_status,
                         1 as is_mirror,
                         to_fetch.guid_idx as guid_idx
                     FROM loginsM
                     JOIN to_fetch
                         ON loginsM.guid = to_fetch.fetch_guid

                     UNION ALL

                     SELECT
                         {common_cols},
                         NULL as is_overridden,
                         NULL as server_modified,
                         local_modified,
                         is_deleted,
                         sync_status,
                         0 as is_mirror,
                         to_fetch.guid_idx as guid_idx
                     FROM loginsL
                     JOIN to_fetch
                         ON loginsL.guid = to_fetch.fetch_guid",
                    // give each VALUES item 2 entries, an index and the parameter.
                    vals = values_with_idx,
                    common_cols = schema::COMMON_COLS,
                );

                let db = &self.store.db.lock();
                let mut stmt = db.prepare(&query)?;

                let rows = stmt.query_and_then(rusqlite::params_from_iter(chunk), |row| {
                    let guid_idx_i = row.get::<_, i64>("guid_idx")?;
                    // Hitting this means our math is wrong...
                    assert!(guid_idx_i >= 0);

                    let guid_idx = guid_idx_i as usize;
                    let is_mirror: bool = row.get("is_mirror")?;
                    if is_mirror {
                        sync_data[guid_idx].set_mirror(MirrorLogin::from_row(row)?)?;
                    } else {
                        sync_data[guid_idx].set_local(LocalLogin::from_row(row)?)?;
                    }
                    self.scope.err_if_interrupted()?;
                    Ok(())
                })?;
                // `rows` is an Iterator<Item = Result<()>>, so we need to collect to handle the errors.
                rows.collect::<Result<_>>()?;
                Ok(())
            },
        )?;
        Ok(sync_data)
    }

    fn fetch_outgoing(&self) -> Result<Vec<OutgoingBso>> {
        // Taken from iOS. Arbitrarily large, so that clients that want to
        // process deletions first can; for us it doesn't matter.
        const TOMBSTONE_SORTINDEX: i32 = 5_000_000;
        const DEFAULT_SORTINDEX: i32 = 1;
        let db = self.store.db.lock();
        let mut stmt = db.prepare_cached(&format!(
            "SELECT L.*, M.enc_unknown_fields
             FROM loginsL L LEFT JOIN loginsM M ON L.guid = M.guid
             WHERE sync_status IS NOT {synced}",
            synced = SyncStatus::Synced as u8
        ))?;
        let bsos = stmt.query_and_then([], |row| {
            self.scope.err_if_interrupted()?;
            Ok(if row.get::<_, bool>("is_deleted")? {
                let envelope = OutgoingEnvelope {
                    id: row.get::<_, String>("guid")?.into(),
                    sortindex: Some(TOMBSTONE_SORTINDEX),
                    ..Default::default()
                };
                OutgoingBso::new_tombstone(envelope)
            } else {
                let unknown = row.get::<_, Option<String>>("enc_unknown_fields")?;
                let mut bso = EncryptedLogin::from_row(row)?.into_bso(self.encdec()?, unknown)?;
                bso.envelope.sortindex = Some(DEFAULT_SORTINDEX);
                bso
            })
        })?;
        bsos.collect::<Result<_>>()
    }

    fn do_apply_incoming(
        &self,
        inbound: Vec<IncomingBso>,
        timestamp: ServerTimestamp,
        telem: &mut telemetry::Engine,
    ) -> Result<Vec<OutgoingBso>> {
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let data = self.fetch_login_data(inbound, &mut incoming_telemetry)?;
        let plan = {
            let result = self.reconcile(data, timestamp, &mut incoming_telemetry);
            telem.incoming(incoming_telemetry);
            result
        }?;
        self.execute_plan(plan)?;
        self.fetch_outgoing()
    }

    fn set_last_sync(&self, db: &LoginDb, last_sync: ServerTimestamp) -> Result<()> {
        log::debug!("Updating last sync to {}", last_sync);
        let last_sync_millis = last_sync.as_millis();
        db.put_meta(schema::LAST_SYNC_META_KEY, &last_sync_millis)
    }

    fn get_last_sync(&self, db: &LoginDb) -> Result<Option<ServerTimestamp>> {
        let millis = db.get_meta::<i64>(schema::LAST_SYNC_META_KEY)?.unwrap();
        Ok(Some(ServerTimestamp(millis)))
    }

    pub fn set_global_state(&self, state: &Option<String>) -> Result<()> {
        let to_write = match state {
            Some(ref s) => s,
            None => "",
        };
        let db = self.store.db.lock();
        db.put_meta(schema::GLOBAL_STATE_META_KEY, &to_write)
    }

    pub fn get_global_state(&self) -> Result<Option<String>> {
        let db = self.store.db.lock();
        db.get_meta::<String>(schema::GLOBAL_STATE_META_KEY)
    }

    fn mark_as_synchronized(&self, guids: &[&str], ts: ServerTimestamp) -> Result<()> {
        let db = self.store.db.lock();
        let tx = db.unchecked_transaction()?;
        sql_support::each_chunk(guids, |chunk, _| -> Result<()> {
            db.execute(
                &format!(
                    "DELETE FROM loginsM WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                rusqlite::params_from_iter(chunk),
            )?;
            self.scope.err_if_interrupted()?;

            db.execute(
                &format!(
                    "INSERT OR IGNORE INTO loginsM (
                         {common_cols}, is_overridden, server_modified
                     )
                     SELECT {common_cols}, 0, {modified_ms_i64}
                     FROM loginsL
                     WHERE is_deleted = 0 AND guid IN ({vars})",
                    common_cols = schema::COMMON_COLS,
                    modified_ms_i64 = ts.as_millis(),
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                rusqlite::params_from_iter(chunk),
            )?;
            self.scope.err_if_interrupted()?;

            db.execute(
                &format!(
                    "DELETE FROM loginsL WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                rusqlite::params_from_iter(chunk),
            )?;
            self.scope.err_if_interrupted()?;
            Ok(())
        })?;
        self.set_last_sync(&db, ts)?;
        tx.commit()?;
        Ok(())
    }

    // This exists here as a public function so the store can call it. Ideally
    // the store would not do that :) Then it can go back into the sync trait
    // and return an anyhow::Result
    pub fn do_reset(&self, assoc: &EngineSyncAssociation) -> Result<()> {
        log::info!("Executing reset on password engine!");
        let db = self.store.db.lock();
        let tx = db.unchecked_transaction()?;
        db.execute_all(&[
            &CLONE_ENTIRE_MIRROR_SQL,
            "DELETE FROM loginsM",
            &format!("UPDATE loginsL SET sync_status = {}", SyncStatus::New as u8),
        ])?;
        self.set_last_sync(&db, ServerTimestamp(0))?;
        match assoc {
            EngineSyncAssociation::Disconnected => {
                db.delete_meta(schema::GLOBAL_SYNCID_META_KEY)?;
                db.delete_meta(schema::COLLECTION_SYNCID_META_KEY)?;
            }
            EngineSyncAssociation::Connected(ids) => {
                db.put_meta(schema::GLOBAL_SYNCID_META_KEY, &ids.global)?;
                db.put_meta(schema::COLLECTION_SYNCID_META_KEY, &ids.coll)?;
            }
        };
        db.delete_meta(schema::GLOBAL_STATE_META_KEY)?;
        tx.commit()?;
        Ok(())
    }

    // It would be nice if this were a batch-ish api (e.g. takes a slice of records and finds dupes
    // for each one if they exist)... I can't think of how to write that query, though.
    // This is subtly different from dupe handling by the main API and maybe
    // could be consolidated, but for now it remains sync specific.
    pub(crate) fn find_dupe_login(&self, l: &EncryptedLogin) -> Result<Option<EncryptedLogin>> {
        let form_submit_host_port = l
            .fields
            .form_action_origin
            .as_ref()
            .and_then(|s| util::url_host_port(s));
        let encdec = self.encdec()?;
        let enc_fields = l.decrypt_fields(encdec)?;
        let args = named_params! {
            ":origin": l.fields.origin,
            ":http_realm": l.fields.http_realm,
            ":form_submit": form_submit_host_port,
        };
        let mut query = format!(
            "SELECT {common}
             FROM loginsL
             WHERE origin IS :origin
               AND httpRealm IS :http_realm",
            common = schema::COMMON_COLS,
        );
        if form_submit_host_port.is_some() {
            // Stolen from iOS
            query += " AND (formActionOrigin = '' OR (instr(formActionOrigin, :form_submit) > 0))";
        } else {
            query += " AND formActionOrigin IS :form_submit"
        }
        let db = self.store.db.lock();
        let mut stmt = db.prepare_cached(&query)?;
        for login in stmt
            .query_and_then(args, EncryptedLogin::from_row)?
            .collect::<Result<Vec<EncryptedLogin>>>()?
        {
            let this_enc_fields = login.decrypt_fields(encdec)?;
            if enc_fields.username == this_enc_fields.username {
                return Ok(Some(login));
            }
        }
        Ok(None)
    }
}

impl SyncEngine for LoginsSyncEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "passwords".into()
    }

    fn set_local_encryption_key(&mut self, key: &str) -> anyhow::Result<()> {
        self.encdec = Some(EncryptorDecryptor::new(key)?);
        Ok(())
    }

    fn stage_incoming(
        &self,
        mut inbound: Vec<IncomingBso>,
        _telem: &mut telemetry::Engine,
    ) -> anyhow::Result<()> {
        // We don't have cross-item dependencies like bookmarks does, so we can
        // just apply now instead of "staging"
        self.staged.borrow_mut().append(&mut inbound);
        Ok(())
    }

    fn apply(
        &self,
        timestamp: ServerTimestamp,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<Vec<OutgoingBso>> {
        let inbound = (*self.staged.borrow_mut()).drain(..).collect();
        Ok(self.do_apply_incoming(inbound, timestamp, telem)?)
    }

    fn set_uploaded(&self, new_timestamp: ServerTimestamp, ids: Vec<Guid>) -> anyhow::Result<()> {
        Ok(self.mark_as_synchronized(
            &ids.iter().map(Guid::as_str).collect::<Vec<_>>(),
            new_timestamp,
        )?)
    }

    fn get_collection_request(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Option<CollectionRequest>> {
        let db = self.store.db.lock();
        let since = self.get_last_sync(&db)?.unwrap_or_default();
        Ok(if since == server_timestamp {
            None
        } else {
            Some(
                CollectionRequest::new("passwords".into())
                    .full()
                    .newer_than(since),
            )
        })
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let db = self.store.db.lock();
        let global = db.get_meta(schema::GLOBAL_SYNCID_META_KEY)?;
        let coll = db.get_meta(schema::COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            EngineSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            EngineSyncAssociation::Disconnected
        })
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        self.do_reset(assoc)?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        let db = self.store.db.lock();
        db.wipe(&self.scope)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::insert_login;
    use crate::encryption::test_utils::{TEST_ENCRYPTION_KEY, TEST_ENCRYPTOR};
    use crate::login::test_utils::enc_login;
    use crate::{LoginEntry, LoginFields, RecordFields, SecureLoginFields};
    use std::collections::HashMap;
    use std::sync::Arc;

    // Wrap sync functions for easier testing
    fn run_fetch_login_data(
        store: LoginStore,
        records: Vec<IncomingBso>,
    ) -> (Vec<SyncLoginData>, telemetry::EngineIncoming) {
        let mut engine = LoginsSyncEngine::new(Arc::new(store)).unwrap();
        engine
            .set_local_encryption_key(&TEST_ENCRYPTION_KEY)
            .unwrap();
        let mut telem = sync15::telemetry::EngineIncoming::new();
        (engine.fetch_login_data(records, &mut telem).unwrap(), telem)
    }

    fn run_fetch_outgoing(store: LoginStore) -> Vec<OutgoingBso> {
        let mut engine = LoginsSyncEngine::new(Arc::new(store)).unwrap();
        engine
            .set_local_encryption_key(&TEST_ENCRYPTION_KEY)
            .unwrap();
        engine.fetch_outgoing().unwrap()
    }

    #[test]
    fn test_fetch_login_data() {
        // Test some common cases with fetch_login data
        let store = LoginStore::new_in_memory().unwrap();
        insert_login(&store.db.lock(), "updated_remotely", None, Some("password"));
        insert_login(&store.db.lock(), "deleted_remotely", None, Some("password"));
        insert_login(
            &store.db.lock(),
            "three_way_merge",
            Some("new-local-password"),
            Some("password"),
        );

        let (res, _) = run_fetch_login_data(
            store,
            vec![
                IncomingBso::new_test_tombstone(Guid::new("deleted_remotely")),
                enc_login("added_remotely", "password")
                    .into_bso(&TEST_ENCRYPTOR, None)
                    .unwrap()
                    .to_test_incoming(),
                enc_login("updated_remotely", "new-password")
                    .into_bso(&TEST_ENCRYPTOR, None)
                    .unwrap()
                    .to_test_incoming(),
                enc_login("three_way_merge", "new-remote-password")
                    .into_bso(&TEST_ENCRYPTOR, None)
                    .unwrap()
                    .to_test_incoming(),
            ],
        );
        // For simpler testing, extract/decrypt passwords and put them in a hash map
        #[derive(Debug, PartialEq)]
        struct SyncPasswords {
            local: Option<String>,
            mirror: Option<String>,
            inbound: Option<String>,
        }
        let extracted_passwords: HashMap<String, SyncPasswords> = res
            .into_iter()
            .map(|sync_login_data| {
                let mut guids_seen = HashSet::new();
                let passwords = SyncPasswords {
                    local: sync_login_data.local.map(|local_login| {
                        guids_seen.insert(local_login.login.record.id.clone());
                        local_login
                            .login
                            .decrypt_fields(&TEST_ENCRYPTOR)
                            .unwrap()
                            .password
                    }),
                    mirror: sync_login_data.mirror.map(|mirror_login| {
                        guids_seen.insert(mirror_login.login.record.id.clone());
                        mirror_login
                            .login
                            .decrypt_fields(&TEST_ENCRYPTOR)
                            .unwrap()
                            .password
                    }),
                    inbound: sync_login_data.inbound.map(|incoming| {
                        guids_seen.insert(incoming.login.record.id.clone());
                        incoming
                            .login
                            .decrypt_fields(&TEST_ENCRYPTOR)
                            .unwrap()
                            .password
                    }),
                };
                (guids_seen.into_iter().next().unwrap(), passwords)
            })
            .collect();

        assert_eq!(extracted_passwords.len(), 4);
        assert_eq!(
            extracted_passwords.get("added_remotely").unwrap(),
            &SyncPasswords {
                local: None,
                mirror: None,
                inbound: Some("password".into()),
            }
        );
        assert_eq!(
            extracted_passwords.get("updated_remotely").unwrap(),
            &SyncPasswords {
                local: None,
                mirror: Some("password".into()),
                inbound: Some("new-password".into()),
            }
        );
        assert_eq!(
            extracted_passwords.get("deleted_remotely").unwrap(),
            &SyncPasswords {
                local: None,
                mirror: Some("password".into()),
                inbound: None,
            }
        );
        assert_eq!(
            extracted_passwords.get("three_way_merge").unwrap(),
            &SyncPasswords {
                local: Some("new-local-password".into()),
                mirror: Some("password".into()),
                inbound: Some("new-remote-password".into()),
            }
        );
    }

    #[test]
    fn test_fetch_outgoing() {
        let store = LoginStore::new_in_memory().unwrap();
        insert_login(
            &store.db.lock(),
            "changed",
            Some("new-password"),
            Some("password"),
        );
        insert_login(&store.db.lock(), "unchanged", None, Some("password"));
        insert_login(&store.db.lock(), "added", Some("password"), None);
        insert_login(&store.db.lock(), "deleted", None, Some("password"));
        store.db.lock().delete("deleted").unwrap();

        let changeset = run_fetch_outgoing(store);
        let changes: HashMap<String, serde_json::Value> = changeset
            .into_iter()
            .map(|b| {
                (
                    b.envelope.id.to_string(),
                    serde_json::from_str(&b.payload).unwrap(),
                )
            })
            .collect();
        assert_eq!(changes.len(), 3);
        assert_eq!(changes["added"].get("password").unwrap(), "password");
        assert_eq!(changes["changed"].get("password").unwrap(), "new-password");
        assert!(changes["deleted"].get("deleted").is_some());
        assert!(changes["added"].get("deleted").is_none());
        assert!(changes["changed"].get("deleted").is_none());
    }

    #[test]
    fn test_bad_record() {
        let store = LoginStore::new_in_memory().unwrap();
        for id in ["dummy_000001", "dummy_000002", "dummy_000003"] {
            insert_login(&store.db.lock(), id, Some("password"), Some("password"));
        }
        let (res, telem) = run_fetch_login_data(
            store,
            vec![
                IncomingBso::new_test_tombstone(Guid::new("dummy_000001")),
                // invalid
                IncomingBso::from_test_content(serde_json::json!({
                    "id": "dummy_000002",
                    "garbage": "data",
                    "etc": "not a login"
                })),
                // valid
                IncomingBso::from_test_content(serde_json::json!({
                    "id": "dummy_000003",
                    "formSubmitURL": "https://www.example.com/submit",
                    "hostname": "https://www.example.com",
                    "username": "test",
                    "password": "test",
                })),
            ],
        );
        assert_eq!(telem.get_failed(), 1);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].guid, "dummy_000001");
        assert_eq!(res[1].guid, "dummy_000003");
    }

    fn make_enc_login(
        username: &str,
        password: &str,
        fao: Option<String>,
        realm: Option<String>,
    ) -> EncryptedLogin {
        EncryptedLogin {
            record: RecordFields {
                id: Guid::random().to_string(),
                ..Default::default()
            },
            fields: LoginFields {
                form_action_origin: fao,
                http_realm: realm,
                origin: "http://not-relevant-here.com".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: username.into(),
                password: password.into(),
            }
            .encrypt(&TEST_ENCRYPTOR)
            .unwrap(),
        }
    }

    #[test]
    fn find_dupe_login() {
        let store = LoginStore::new_in_memory().unwrap();

        let to_add = LoginEntry {
            fields: LoginFields {
                form_action_origin: Some("https://www.example.com".into()),
                origin: "http://not-relevant-here.com".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test".into(),
                password: "test".into(),
            },
        };
        let first_id = store
            .add(to_add, &TEST_ENCRYPTION_KEY)
            .expect("should insert first")
            .record
            .id;

        let to_add = LoginEntry {
            fields: LoginFields {
                form_action_origin: Some("https://www.example1.com".into()),
                origin: "http://not-relevant-here.com".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test1".into(),
                password: "test1".into(),
            },
        };
        let second_id = store
            .add(to_add, &TEST_ENCRYPTION_KEY)
            .expect("should insert second")
            .record
            .id;

        let to_add = LoginEntry {
            fields: LoginFields {
                http_realm: Some("http://some-realm.com".into()),
                origin: "http://not-relevant-here.com".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "test1".into(),
                password: "test1".into(),
            },
        };
        let no_form_origin_id = store
            .add(to_add, &TEST_ENCRYPTION_KEY)
            .expect("should insert second")
            .record
            .id;

        let mut engine = LoginsSyncEngine::new(Arc::new(store)).unwrap();
        engine
            .set_local_encryption_key(&TEST_ENCRYPTION_KEY)
            .unwrap();

        let to_find = make_enc_login("test", "test", Some("https://www.example.com".into()), None);
        assert_eq!(
            engine
                .find_dupe_login(&to_find)
                .expect("should work")
                .expect("should be Some()")
                .record
                .id,
            first_id
        );

        let to_find = make_enc_login(
            "test",
            "test",
            Some("https://something-else.com".into()),
            None,
        );
        assert!(engine
            .find_dupe_login(&to_find)
            .expect("should work")
            .is_none());

        let to_find = make_enc_login(
            "test1",
            "test1",
            Some("https://www.example1.com".into()),
            None,
        );
        assert_eq!(
            engine
                .find_dupe_login(&to_find)
                .expect("should work")
                .expect("should be Some()")
                .record
                .id,
            second_id
        );

        let to_find = make_enc_login(
            "other",
            "other",
            Some("https://www.example1.com".into()),
            None,
        );
        assert!(engine
            .find_dupe_login(&to_find)
            .expect("should work")
            .is_none());

        // no form origin.
        let to_find = make_enc_login("test1", "test1", None, Some("http://some-realm.com".into()));
        assert_eq!(
            engine
                .find_dupe_login(&to_find)
                .expect("should work")
                .expect("should be Some()")
                .record
                .id,
            no_form_origin_id
        );
    }

    #[test]
    fn test_roundtrip_unknown() {
        // A couple of helpers
        fn apply_incoming_payload(engine: &LoginsSyncEngine, payload: serde_json::Value) {
            let bso = IncomingBso::from_test_content(payload);
            let mut telem = sync15::telemetry::Engine::new(engine.collection_name());
            engine.stage_incoming(vec![bso], &mut telem).unwrap();
            engine
                .apply(ServerTimestamp::from_millis(0), &mut telem)
                .unwrap();
        }

        fn get_outgoing_payload(engine: &LoginsSyncEngine) -> serde_json::Value {
            // Edit it so it's considered outgoing.
            engine
                .store
                .update(
                    "dummy_000001",
                    LoginEntry {
                        fields: LoginFields {
                            origin: "https://www.example2.com".into(),
                            http_realm: Some("https://www.example2.com".into()),
                            ..Default::default()
                        },
                        sec_fields: SecureLoginFields {
                            username: "test".into(),
                            password: "test".into(),
                        },
                    },
                    &TEST_ENCRYPTION_KEY,
                )
                .unwrap();
            let changeset = engine.fetch_outgoing().unwrap();
            assert_eq!(changeset.len(), 1);
            serde_json::from_str::<serde_json::Value>(&changeset[0].payload).unwrap()
        }

        // The test itself...
        let store = LoginStore::new_in_memory().unwrap();
        let mut engine = LoginsSyncEngine::new(Arc::new(store)).unwrap();
        engine
            .set_local_encryption_key(&TEST_ENCRYPTION_KEY)
            .unwrap();

        apply_incoming_payload(
            &engine,
            serde_json::json!({
                "id": "dummy_000001",
                "formSubmitURL": "https://www.example.com/submit",
                "hostname": "https://www.example.com",
                "username": "test",
                "password": "test",
                "unknown1": "?",
                "unknown2": {"sub": "object"},
            }),
        );

        let payload = get_outgoing_payload(&engine);

        // The outgoing payload for our item should have the unknown fields.
        assert_eq!(payload.get("unknown1").unwrap().as_str().unwrap(), "?");
        assert_eq!(
            payload.get("unknown2").unwrap(),
            &serde_json::json!({"sub": "object"})
        );

        // test mirror updates - record is already in our mirror, but now it's
        // incoming with different unknown fields.
        apply_incoming_payload(
            &engine,
            serde_json::json!({
                "id": "dummy_000001",
                "formSubmitURL": "https://www.example.com/submit",
                "hostname": "https://www.example.com",
                "username": "test",
                "password": "test",
                "unknown2": 99,
                "unknown3": {"something": "else"},
            }),
        );
        let payload = get_outgoing_payload(&engine);
        // old unknown values were replaced.
        assert!(payload.get("unknown1").is_none());
        assert_eq!(payload.get("unknown2").unwrap().as_u64().unwrap(), 99);
        assert_eq!(
            payload
                .get("unknown3")
                .unwrap()
                .as_object()
                .unwrap()
                .get("something")
                .unwrap()
                .as_str()
                .unwrap(),
            "else"
        );
    }
}
