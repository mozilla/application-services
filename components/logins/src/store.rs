/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::db::LoginDb;
use crate::error::*;
use crate::login::{DecryptedLogin, Login, LoginFields, LoginPayload};
use crate::LoginsSyncEngine;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use sync15::{sync_multiple, EngineSyncAssociation, MemoryCachedState, SyncEngine};

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff - needed
    //        to wrap the RefCell as they aren't `Sync`
    static ref STORE_FOR_MANAGER: Mutex<Weak<LoginStore>> = Mutex::new(Weak::new());
}

/// Called by the sync manager to get a sync engine via the store previously
/// registered with the sync manager.
pub fn get_registered_sync_engine(name: &str) -> Option<Box<dyn SyncEngine>> {
    let weak = STORE_FOR_MANAGER.lock().unwrap();
    match weak.upgrade() {
        None => None,
        Some(store) => match name {
            "logins" => Some(Box::new(LoginsSyncEngine::new(Arc::clone(&store)))),
            // panicing here seems reasonable - it's a static error if this
            // it hit, not something that runtime conditions can influence.
            _ => unreachable!("can't provide unknown engine: {}", name),
        },
    }
}

pub struct LoginStore {
    pub db: Mutex<LoginDb>,
}

impl LoginStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Mutex::new(LoginDb::open(path)?);
        Ok(Self { db })
    }

    pub fn new_old(path: impl AsRef<Path>, encryption_key: &str) -> Result<Self> {
        let db = Mutex::new(LoginDb::open_old(path, Some(encryption_key))?);
        Ok(Self { db })
    }

    pub fn new_with_salt(path: impl AsRef<Path>, encryption_key: &str, salt: &str) -> Result<Self> {
        let db = Mutex::new(LoginDb::open_with_salt(path, encryption_key, salt)?);
        Ok(Self { db })
    }

    pub fn new_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        let db = Mutex::new(LoginDb::open_in_memory(encryption_key)?);
        Ok(Self { db })
    }

    pub fn decrypt_and_fixup_login(&self, _enc_key: &str, _login: Login) -> Result<DecryptedLogin> {
        Ok(DecryptedLogin {
            ..Default::default()
        })
    }

    pub fn list(&self) -> Result<Vec<Login>> {
        Ok(Vec::new())
    }

    pub fn list_old(&self) -> Result<Vec<LoginPayload>> {
        self.db.lock().unwrap().get_all()
    }

    pub fn get(&self, _id: &str) -> Result<Option<Login>> {
        Ok(None)
    }

    pub fn get_old(&self, id: &str) -> Result<Option<LoginPayload>> {
        self.db.lock().unwrap().get_by_id(id)
    }

    pub fn get_by_base_domain(&self, _base_domain: &str) -> Result<Vec<Login>> {
        Ok(Vec::new())
    }

    pub fn get_by_base_domain_old(&self, base_domain: &str) -> Result<Vec<LoginPayload>> {
        self.db.lock().unwrap().get_by_base_domain(base_domain)
    }

    pub fn potential_dupes_ignoring_username(
        &self,
        login: LoginPayload,
    ) -> Result<Vec<LoginPayload>> {
        self.db
            .lock()
            .unwrap()
            .potential_dupes_ignoring_username(&login)
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        self.db.lock().unwrap().touch(id)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.lock().unwrap().delete(id)
    }

    pub fn wipe(&self) -> Result<()> {
        // This should not be exposed - it wipes the server too and there's
        // no good reason to expose that to consumers. wipe_local makes some
        // sense though.
        // TODO: this is exposed to android-components consumers - we should
        // check if anyone actually calls it.
        let db = self.db.lock().unwrap();
        let scope = db.begin_interrupt_scope();
        db.wipe(&scope)?;
        Ok(())
    }

    pub fn wipe_local(&self) -> Result<()> {
        self.db.lock().unwrap().wipe_local()?;
        Ok(())
    }

    pub fn reset(self: Arc<Self>) -> Result<()> {
        // Reset should not exist here - all resets should be done via the
        // sync manager. It seems that actual consumers don't use this, but
        // some tests do, so it remains for now.
        let engine = LoginsSyncEngine::new(Arc::clone(&self));
        engine.do_reset(&EngineSyncAssociation::Disconnected)?;
        Ok(())
    }

    pub fn update(&self, login: LoginPayload) -> Result<()> {
        self.db.lock().unwrap().update(login)
    }

    pub fn add_or_update(&self, _enc_key: &str, _login: LoginFields) -> Result<String> {
        Ok(String::default())
    }

    pub fn add(&self, login: LoginPayload) -> Result<String> {
        // Just return the record's ID (which we may have generated).
        self.db
            .lock()
            .unwrap()
            .add(login)
            .map(|record| record.guid().into_string())
    }

    pub fn import_multiple(&self, _enc_key: &str, _logins: Vec<Login>) -> Result<String> {
        Ok(String::default())
    }

    pub fn import_multiple_old(&self, logins: Vec<LoginPayload>) -> Result<String> {
        let metrics = self.db.lock().unwrap().import_multiple(&logins)?;
        Ok(serde_json::to_string(&metrics)?)
    }

    pub fn disable_mem_security(&self) -> Result<()> {
        self.db.lock().unwrap().disable_mem_security()
    }

    pub fn new_interrupt_handle(&self) -> sql_support::SqlInterruptHandle {
        self.db.lock().unwrap().new_interrupt_handle()
    }

    pub fn rekey_database(&self, new_encryption_key: &str) -> Result<()> {
        self.db.lock().unwrap().rekey_database(new_encryption_key)
    }

    pub fn check_valid_with_no_dupes(&self, login: &LoginPayload) -> Result<()> {
        self.db.lock().unwrap().check_valid_with_no_dupes(&login)
    }

    /// A convenience wrapper around sync_multiple.
    // Unfortunately, iOS still uses this until they use the sync manager
    // This can almost die later - consumers should never call it (they should
    // use the sync manager) and any of our examples probably can too!
    // Once this dies, `mem_cached_state` can die too.
    pub fn sync(
        self: Arc<Self>,
        key_id: String,
        access_token: String,
        sync_key: String,
        tokenserver_url: String,
    ) -> Result<String> {
        let engine = LoginsSyncEngine::new(Arc::clone(&self));

        // This is a bit hacky but iOS still uses sync() and we can only pass strings over ffi
        // Below was ported from the "C" ffi code that does essentially the same thing
        let storage_init = &sync15::Sync15StorageClientInit {
            key_id,
            access_token,
            tokenserver_url: url::Url::parse(tokenserver_url.as_str())?,
        };
        let root_sync_key = &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?;

        let mut disk_cached_state = engine.get_global_state()?;
        let mut mem_cached_state = MemoryCachedState::default();

        let mut result = sync_multiple(
            &[&engine],
            &mut disk_cached_state,
            &mut mem_cached_state,
            storage_init,
            root_sync_key,
            &engine.scope,
            None,
        );
        // We always update the state - sync_multiple does the right thing
        // if it needs to be dropped (ie, they will be None or contain Nones etc)
        engine.set_global_state(&disk_cached_state)?;

        // for b/w compat reasons, we do some dances with the result.
        // XXX - note that this means telemetry isn't going to be reported back
        // to the app - we need to check with lockwise about whether they really
        // need these failures to be reported or whether we can loosen this.
        if let Err(e) = result.result {
            return Err(e.into());
        }
        match result.engine_results.remove("passwords") {
            None | Some(Ok(())) => Ok(serde_json::to_string(&result.telemetry).unwrap()),
            Some(Err(e)) => Err(e.into()),
        }
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    pub fn register_with_sync_manager(self: Arc<Self>) {
        let mut state = STORE_FOR_MANAGER.lock().unwrap();
        *state = Arc::downgrade(&self);
    }

    // this isn't exposed by uniffi - currently the
    // only consumer of this is our "example" (and hence why they
    // are `pub` and not `pub(crate)`).
    // We could probably make the example work with the sync manager - but then
    // our example would link with places and logins etc, and it's not a big
    // deal really.
    pub fn create_logins_sync_engine(self: Arc<Self>) -> Box<dyn SyncEngine> {
        Box::new(LoginsSyncEngine::new(self))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util;
    use more_asserts::*;
    use std::cmp::Reverse;
    use std::time::SystemTime;
    // Doesn't check metadata fields
    fn assert_logins_equiv(a: &LoginPayload, b: &LoginPayload) {
        assert_eq!(b.guid(), a.guid());
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
        let store = LoginStore::new_in_memory(Some("secret")).unwrap();
        let list = store.list_old().expect("Grabbing Empty list to work");
        assert_eq!(list.len(), 0);
        let start_us = util::system_time_ms_i64(SystemTime::now());

        let a = LoginPayload {
            id: "aaaaaaaaaaaa".into(),
            hostname: "https://www.example.com".into(),
            form_submit_url: Some("https://www.example.com".into()),
            username: "coolperson21".into(),
            password: "p4ssw0rd".into(),
            username_field: "user_input".into(),
            password_field: "pass_input".into(),
            ..LoginPayload::default()
        };

        let b = LoginPayload {
            // Note: no ID, should be autogenerated for us
            hostname: "https://www.example2.com".into(),
            http_realm: Some("Some String Here".into()),
            username: "asdf".into(),
            password: "fdsa".into(),
            ..LoginPayload::default()
        };
        let a_id = store.add(a.clone()).expect("added a");
        let b_id = store.add(b.clone()).expect("added b");

        assert_eq!(a_id, a.guid());

        assert_ne!(b_id, b.guid(), "Should generate guid when none provided");

        let a_from_db = store
            .get_old(&a_id)
            .expect("Not to error getting a")
            .expect("a to exist");

        assert_logins_equiv(&a, &a_from_db);
        assert_ge!(a_from_db.time_created, start_us);
        assert_ge!(a_from_db.time_password_changed, start_us);
        assert_ge!(a_from_db.time_last_used, start_us);
        assert_eq!(a_from_db.times_used, 1);

        let b_from_db = store
            .get_old(&b_id)
            .expect("Not to error getting b")
            .expect("b to exist");

        assert_logins_equiv(
            &b_from_db,
            &LoginPayload {
                id: b_id.to_string(),
                ..b.clone()
            },
        );
        assert_ge!(b_from_db.time_created, start_us);
        assert_ge!(b_from_db.time_password_changed, start_us);
        assert_ge!(b_from_db.time_last_used, start_us);
        assert_eq!(b_from_db.times_used, 1);

        let mut list = store.list_old().expect("Grabbing list to work");
        assert_eq!(list.len(), 2);

        let mut expect = vec![a_from_db, b_from_db.clone()];

        list.sort_by_key(|b| Reverse(b.guid()));
        expect.sort_by_key(|b| Reverse(b.guid()));
        assert_eq!(list, expect);

        store.delete(&a_id).expect("Successful delete");
        assert!(store
            .get(&a_id)
            .expect("get after delete should still work")
            .is_none());

        let list = store.list_old().expect("Grabbing list to work");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], b_from_db);

        let list = store
            .get_by_base_domain_old("example2.com")
            .expect("Expect a list for this hostname");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], b_from_db);

        let list = store
            .get_by_base_domain_old("www.example.com")
            .expect("Expect an empty list");
        assert_eq!(list.len(), 0);

        let now_us = util::system_time_ms_i64(SystemTime::now());
        let b2 = LoginPayload {
            password: "newpass".into(),
            id: b_id.to_string(),
            ..b
        };

        store.update(b2.clone()).expect("update b should work");

        let b_after_update = store
            .get_old(&b_id)
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

    #[test]
    fn test_rekey() {
        let store = LoginStore::new_in_memory(Some("secret")).unwrap();
        store.rekey_database("new_encryption_key").unwrap();
        let list = store.list_old().expect("Grabbing Empty list to work");
        assert_eq!(list.len(), 0);
    }
    #[test]
    fn test_sync_manager_registration() {
        let store = Arc::new(LoginStore::new_in_memory(Some("sync-manager")).unwrap());
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 0);
        Arc::clone(&store).register_with_sync_manager();
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        let registered = STORE_FOR_MANAGER
            .lock()
            .unwrap()
            .upgrade()
            .expect("should upgrade");
        assert!(Arc::ptr_eq(&store, &registered));
        drop(registered);
        // should be no new references
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        // dropping the registered object should drop the registration.
        drop(store);
        assert!(STORE_FOR_MANAGER.lock().unwrap().upgrade().is_none());
    }
}

#[test]
fn test_send() {
    fn ensure_send<T: Send>() {}
    ensure_send::<LoginStore>();
}
