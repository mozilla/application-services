/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use login::Login;
use error::*;
use sync::{self, Sync15StorageClient, Sync15StorageClientInit, GlobalState, KeyBundle};
use db::LoginDb;
use std::path::Path;
use std::cell::Cell;
use serde_json;
use rusqlite;

#[derive(Debug)]
pub(crate) struct SyncInfo {
    pub state: GlobalState,
    pub client: Sync15StorageClient,
    // Used so that we know whether or not we need to re-initialize `client`
    pub last_client_init: Sync15StorageClientInit,
}

// This isn't really an engine in the firefox sync15 desktop sense -- it's
// really a bundle of state that contains the sync storage client, the sync
// state, and the login DB.
pub struct PasswordEngine {
    sync: Cell<Option<SyncInfo>>,
    db: LoginDb,
}

impl PasswordEngine {

    pub fn new(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        let db = LoginDb::open(path, encryption_key)?;
        Ok(Self { db, sync: Cell::new(None) })
    }

    pub fn new_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        let db = LoginDb::open_in_memory(encryption_key)?;
        Ok(Self { db, sync: Cell::new(None) })
    }

    pub fn list(&self) -> Result<Vec<Login>> {
        self.db.get_all()
    }

    pub fn get(&self, id: &str) -> Result<Option<Login>> {
        self.db.get_by_id(id)
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        self.db.touch(id)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.delete(id)
    }

    pub fn wipe(&self) -> Result<()> {
        self.db.wipe()
    }

    pub fn reset(&self) -> Result<()> {
        self.db.reset()
    }

    pub fn update(&self, login: Login) -> Result<()> {
        self.db.update(login)
    }

    pub fn add(&self, login: Login) -> Result<String> {
        // Just return the record's ID (which we may have generated).
        self.db.add(login).map(|record| record.id)
    }

    // This is basiclaly exposed just for sync_pass_sql, but it doesn't seem
    // unreasonable.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.db.db
    }

    pub fn sync(
        &self,
        storage_init: &Sync15StorageClientInit,
        root_sync_key: &KeyBundle
    ) -> Result<()> {

        // Note: If `to_ready` (or anything else with a ?) fails below, this
        // `replace()` means we end up with `state.sync.is_none()`, which means the
        // next sync will redownload meta/global, crypto/keys, etc. without
        // needing to. Apparently this is both okay and by design.
        let maybe_sync_info = self.sync.replace(None).map(Ok);

        // `maybe_sync_info` is None if we haven't called `sync` since
        // restarting the browser.
        //
        // If this is the case we may or may not have a persisted version of
        // GlobalState stored in the DB (we will iff we've synced before, unless
        // we've `reset()`, which clears it out).
        let mut sync_info = maybe_sync_info.unwrap_or_else(|| -> Result<SyncInfo> {
            info!("First time through since unlock. Trying to load persisted global state.");
            let state = if let Some(persisted_global_state) = self.db.get_global_state()? {
                serde_json::from_str::<GlobalState>(&persisted_global_state)
                .unwrap_or_else(|_| {
                    // Don't log the error since it might contain sensitive
                    // info like keys (the JSON does, after all).
                    error!("Failed to parse GlobalState from JSON! Falling back to default");
                    // Unstick ourselves by using the default state.
                    GlobalState::default()
                })
            } else {
                info!("No previously persisted global state, using default");
                GlobalState::default()
            };
            let client = Sync15StorageClient::new(storage_init.clone())?;
            Ok(SyncInfo {
                state,
                client,
                last_client_init: storage_init.clone(),
            })
        })?;

        // If the options passed for initialization of the storage client aren't
        // the same as the ones we used last time, reinitialize it. (Note that
        // we could avoid the comparison in the case where we had `None` in
        // `state.sync` before, but this probably doesn't matter).
        //
        // It's a little confusing that we do things this way (transparently
        // re-initialize the client), but it reduces the size of the API surface
        // exposed over the FFI, and simplifies the states that the client code
        // has to consider (as far as it's concerned it just has to pass
        // `current` values for these things, and not worry about having to
        // re-initialize the sync state).
        if storage_init != &sync_info.last_client_init {
            info!("Detected change in storage client init, updating");
            sync_info.client = Sync15StorageClient::new(storage_init.clone())?;
            sync_info.last_client_init = storage_init.clone();
        }

        // Advance the state machine to the point where it can perform a full
        // sync. This may involve uploading meta/global, crypto/keys etc.
        {
            // Scope borrow of `sync_info.client`
            let mut state_machine =
                sync::SetupStateMachine::for_full_sync(&sync_info.client, &root_sync_key);
            info!("Advancing state machine to ready (full)");
            let next_sync_state = state_machine.to_ready(sync_info.state)?;
            sync_info.state = next_sync_state;
        }

        // Reset our local state if necessary.
        if sync_info.state.engines_that_need_local_reset().contains("passwords") {
            info!("Passwords sync ID changed; engine needs local reset");
            self.db.reset()?;
        }

        // Persist the current sync state in the DB.
        info!("Updating persisted global state");
        let s = sync_info.state.to_persistable_string();
        self.db.set_global_state(&s)?;

        info!("Syncing passwords engine!");

        let ts = self.db.get_last_sync()?.unwrap_or_default();

        // We don't use `?` here so that we can restore the value of of
        // `self.sync` even if sync fails.
        let result = sync::synchronize(
            &sync_info.client,
            &sync_info.state,
            &self.db,
            "passwords".into(),
            ts,
            true
        );

        match &result {
            Ok(()) => info!("Sync was successful!"),
            Err(e) => warn!("Sync failed! {:?}", e),
        }

        // Restore our value of `sync_info` even if the sync failed.
        self.sync.replace(Some(sync_info));

        Ok(result?)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::SystemTime;
    use util;
    // Doesn't check metadata fields
    fn assert_logins_equiv(a: &Login, b: &Login) {
        assert_eq!(b.id, a.id);
        assert_eq!(b.hostname, a.hostname);
        assert_eq!(b.form_submit_url, a.form_submit_url);
        assert_eq!(b.http_realm, a.http_realm);
        assert_eq!(b.username, a.username);
        assert_eq!(b.password, a.password);
        assert_eq!(b.username_field, a.username_field);
        assert_eq!(b.password_field, a.password_field);
    }

    #[test]
    fn test_general() {
        let engine = PasswordEngine::new_in_memory(Some("secret")).unwrap();
        let list = engine.list().expect("Grabbing Empty list to work");
        assert_eq!(list.len(), 0);
        let start_us = util::system_time_ms_i64(SystemTime::now());

        let a = Login {
            id: "aaaaaaaaaaaa".into(),
            hostname: "https://www.example.com".into(),
            form_submit_url: Some("https://www.example.com/login".into()),
            username: "coolperson21".into(),
            password: "p4ssw0rd".into(),
            username_field: "user_input".into(),
            password_field: "pass_input".into(),
            .. Login::default()
        };

        let b = Login {
            // Note: no ID, should be autogenerated for us
            hostname: "https://www.example2.com".into(),
            http_realm: Some("Some String Here".into()),
            username: "asdf".into(),
            password: "fdsa".into(),
            username_field: "input_user".into(),
            password_field: "input_pass".into(),
            .. Login::default()
        };

        let a_id = engine.add(a.clone()).expect("added a");
        let b_id = engine.add(b.clone()).expect("added b");

        assert_eq!(a_id, a.id);

        assert_ne!(b_id, b.id, "Should generate guid when none provided");

        let a_from_db = engine.get(&a_id)
            .expect("Not to error getting a")
            .expect("a to exist");

        assert_logins_equiv(&a, &a_from_db);
        assert_ge!(a_from_db.time_created, start_us);
        assert_ge!(a_from_db.time_password_changed, start_us);
        assert_ge!(a_from_db.time_last_used, start_us);
        assert_eq!(a_from_db.times_used, 1);

        let b_from_db = engine.get(&b_id)
            .expect("Not to error getting b")
            .expect("b to exist");

        assert_logins_equiv(&b_from_db, &Login {
            id: b_id.clone(),
            .. b.clone()
        });
        assert_ge!(b_from_db.time_created, start_us);
        assert_ge!(b_from_db.time_password_changed, start_us);
        assert_ge!(b_from_db.time_last_used, start_us);
        assert_eq!(b_from_db.times_used, 1);

        let mut list = engine.list().expect("Grabbing list to work");
        assert_eq!(list.len(), 2);
        let mut expect = vec![a_from_db.clone(), b_from_db.clone()];

        list.sort_by(|a, b| b.id.cmp(&a.id));
        expect.sort_by(|a, b| b.id.cmp(&a.id));
        assert_eq!(list, expect);

        engine.delete(&a_id).expect("Successful delete");
        assert!(engine.get(&a_id)
            .expect("get after delete should still work")
            .is_none());

        let list = engine.list().expect("Grabbing list to work");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], b_from_db);

        let now_us = util::system_time_ms_i64(SystemTime::now());
        let b2 = Login { password: "newpass".into(), id: b_id.clone(), .. b.clone() };

        engine.update(b2.clone()).expect("update b should work");

        let b_after_update = engine.get(&b_id)
            .expect("Not to error getting b")
            .expect("b to exist");

        assert_logins_equiv(&b_after_update, &b2);
        assert_ge!(b_after_update.time_created, start_us);
        assert_le!(b_after_update.time_created, now_us);
        assert_ge!(b_after_update.time_password_changed, now_us);
        assert_ge!(b_after_update.time_last_used, now_us);
        // Should be two even though we updated twice
        assert_eq!(b_after_update.times_used, 2);
    }
}
