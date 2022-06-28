/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::db::LoginDb;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::login::{EncryptedLogin, Login, LoginEntry};
use crate::LoginsSyncEngine;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::{Arc, Weak};
use sync15::{sync_multiple, EngineSyncAssociation, MemoryCachedState, SyncEngine, SyncEngineId};

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff - needed
    //        to wrap the RefCell as they aren't `Sync`
    static ref STORE_FOR_MANAGER: Mutex<Weak<LoginStore>> = Mutex::new(Weak::new());
}

/// Called by the sync manager to get a sync engine via the store previously
/// registered with the sync manager.
pub fn get_registered_sync_engine(engine_id: &SyncEngineId) -> Option<Box<dyn SyncEngine>> {
    let weak = STORE_FOR_MANAGER.lock();
    match weak.upgrade() {
        None => None,
        Some(store) => match create_sync_engine(store, engine_id) {
            Ok(engine) => Some(engine),
            Err(e) => {
                log::error!("logins: get_registered_sync_engine: {}", e);
                None
            }
        },
    }
}

fn create_sync_engine(
    store: Arc<LoginStore>,
    engine_id: &SyncEngineId,
) -> Result<Box<dyn SyncEngine>> {
    match engine_id {
        SyncEngineId::Passwords => Ok(Box::new(LoginsSyncEngine::new(Arc::clone(&store))?)),
        // panicing here seems reasonable - it's a static error if this
        // it hit, not something that runtime conditions can influence.
        _ => unreachable!("can't provide unknown engine: {}", engine_id),
    }
}

pub struct LoginStore {
    pub db: Mutex<LoginDb>,
}

impl LoginStore {
    pub fn new(path: impl AsRef<Path>) -> ApiResult<Self> {
        handle_error! {
            let db = Mutex::new(LoginDb::open(path)?);
            Ok(Self { db })
        }
    }

    pub fn new_from_db(db: LoginDb) -> Self {
        Self { db: Mutex::new(db) }
    }

    pub fn new_in_memory() -> ApiResult<Self> {
        handle_error! {
            let db = Mutex::new(LoginDb::open_in_memory()?);
            Ok(Self { db })
        }
    }

    pub fn list(&self) -> ApiResult<Vec<EncryptedLogin>> {
        handle_error! {
            self.db.lock().get_all()
        }
    }

    pub fn get(&self, id: &str) -> ApiResult<Option<EncryptedLogin>> {
        handle_error! {
            self.db.lock().get_by_id(id)
        }
    }

    pub fn get_by_base_domain(&self, base_domain: &str) -> ApiResult<Vec<EncryptedLogin>> {
        handle_error! {
            self.db.lock().get_by_base_domain(base_domain)
        }
    }

    pub fn find_login_to_update(
        &self,
        entry: LoginEntry,
        enc_key: &str,
    ) -> ApiResult<Option<Login>> {
        handle_error! {
            let encdec = EncryptorDecryptor::new(enc_key)?;
            self.db.lock().find_login_to_update(entry, &encdec)
        }
    }

    pub fn touch(&self, id: &str) -> ApiResult<()> {
        handle_error! {
            self.db.lock().touch(id)
        }
    }

    pub fn delete(&self, id: &str) -> ApiResult<bool> {
        handle_error! {
            self.db.lock().delete(id)
        }
    }

    pub fn wipe(&self) -> ApiResult<()> {
        // This should not be exposed - it wipes the server too and there's
        // no good reason to expose that to consumers. wipe_local makes some
        // sense though.
        // TODO: this is exposed to android-components consumers - we should
        // check if anyone actually calls it.
        handle_error! {
            let db = self.db.lock();
            let scope = db.begin_interrupt_scope()?;
            db.wipe(&scope)?;
            Ok(())
        }
    }

    pub fn wipe_local(&self) -> ApiResult<()> {
        handle_error! {
            self.db.lock().wipe_local()?;
            Ok(())
        }
    }

    pub fn reset(self: Arc<Self>) -> ApiResult<()> {
        // Reset should not exist here - all resets should be done via the
        // sync manager. It seems that actual consumers don't use this, but
        // some tests do, so it remains for now.
        handle_error! {
            let engine = LoginsSyncEngine::new(Arc::clone(&self))?;
            engine.do_reset(&EngineSyncAssociation::Disconnected)?;
            Ok(())
        }
    }

    pub fn update(&self, id: &str, entry: LoginEntry, enc_key: &str) -> ApiResult<EncryptedLogin> {
        handle_error! {
            let encdec = EncryptorDecryptor::new(enc_key)?;
            self.db.lock().update(id, entry, &encdec)
        }
    }

    pub fn add(&self, entry: LoginEntry, enc_key: &str) -> ApiResult<EncryptedLogin> {
        handle_error! {
            let encdec = EncryptorDecryptor::new(enc_key)?;
            self.db.lock().add(entry, &encdec)
        }
    }

    pub fn add_or_update(&self, entry: LoginEntry, enc_key: &str) -> ApiResult<EncryptedLogin> {
        handle_error! {
            let encdec = EncryptorDecryptor::new(enc_key)?;
            self.db.lock().add_or_update(entry, &encdec)
        }
    }

    pub fn import_multiple(&self, logins: Vec<Login>, enc_key: &str) -> ApiResult<String> {
        handle_error! {
            let encdec = EncryptorDecryptor::new(enc_key)?;
            let metrics = self.db.lock().import_multiple(logins, &encdec)?;
            Ok(serde_json::to_string(&metrics)?)
        }
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
        local_encryption_key: String,
    ) -> ApiResult<String> {
        handle_error! {
            let mut engine = LoginsSyncEngine::new(Arc::clone(&self))?;
            engine
                .set_local_encryption_key(&local_encryption_key)
                .unwrap();

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
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    pub fn register_with_sync_manager(self: Arc<Self>) {
        let mut state = STORE_FOR_MANAGER.lock();
        *state = Arc::downgrade(&self);
    }

    // this isn't exposed by uniffi - currently the
    // only consumer of this is our "example" (and hence why they
    // are `pub` and not `pub(crate)`).
    // We could probably make the example work with the sync manager - but then
    // our example would link with places and logins etc, and it's not a big
    // deal really.
    pub fn create_logins_sync_engine(self: Arc<Self>) -> ApiResult<Box<dyn SyncEngine>> {
        handle_error! {
            Ok(Box::new(LoginsSyncEngine::new(self)?) as Box<dyn SyncEngine>)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::encryption::test_utils::{TEST_ENCRYPTION_KEY, TEST_ENCRYPTOR};
    use crate::util;
    use crate::{LoginFields, SecureLoginFields};
    use more_asserts::*;
    use std::cmp::Reverse;
    use std::time::SystemTime;

    fn assert_logins_equiv(a: &LoginEntry, b: &EncryptedLogin) {
        let b_e = b.decrypt_fields(&TEST_ENCRYPTOR).unwrap();
        assert_eq!(a.fields, b.fields);
        assert_eq!(b_e.username, a.sec_fields.username);
        assert_eq!(b_e.password, a.sec_fields.password);
    }

    #[test]
    fn test_general() {
        let store = LoginStore::new_in_memory().unwrap();
        let list = store.list().expect("Grabbing Empty list to work");
        assert_eq!(list.len(), 0);
        let start_us = util::system_time_ms_i64(SystemTime::now());

        let a = LoginEntry {
            fields: LoginFields {
                origin: "https://www.example.com".into(),
                form_action_origin: Some("https://www.example.com".into()),
                username_field: "user_input".into(),
                password_field: "pass_input".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "coolperson21".into(),
                password: "p4ssw0rd".into(),
            },
        };

        let b = LoginEntry {
            fields: LoginFields {
                origin: "https://www.example2.com".into(),
                http_realm: Some("Some String Here".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "asdf".into(),
                password: "fdsa".into(),
            },
        };
        let a_id = store
            .add(a.clone(), &TEST_ENCRYPTION_KEY)
            .expect("added a")
            .record
            .id;
        let b_id = store
            .add(b.clone(), &TEST_ENCRYPTION_KEY)
            .expect("added b")
            .record
            .id;

        let a_from_db = store
            .get(&a_id)
            .expect("Not to error getting a")
            .expect("a to exist");

        assert_logins_equiv(&a, &a_from_db);
        assert_ge!(a_from_db.record.time_created, start_us);
        assert_ge!(a_from_db.record.time_password_changed, start_us);
        assert_ge!(a_from_db.record.time_last_used, start_us);
        assert_eq!(a_from_db.record.times_used, 1);

        let b_from_db = store
            .get(&b_id)
            .expect("Not to error getting b")
            .expect("b to exist");

        assert_logins_equiv(&LoginEntry { ..b.clone() }, &b_from_db);
        assert_ge!(b_from_db.record.time_created, start_us);
        assert_ge!(b_from_db.record.time_password_changed, start_us);
        assert_ge!(b_from_db.record.time_last_used, start_us);
        assert_eq!(b_from_db.record.times_used, 1);

        let mut list = store.list().expect("Grabbing list to work");
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

        let list = store.list().expect("Grabbing list to work");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], b_from_db);

        let list = store
            .get_by_base_domain("example2.com")
            .expect("Expect a list for this origin");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], b_from_db);

        let list = store
            .get_by_base_domain("www.example.com")
            .expect("Expect an empty list");
        assert_eq!(list.len(), 0);

        let now_us = util::system_time_ms_i64(SystemTime::now());
        let b2 = LoginEntry {
            sec_fields: SecureLoginFields {
                username: b.sec_fields.username.to_owned(),
                password: "newpass".into(),
            },
            ..b
        };

        store
            .update(&b_id, b2.clone(), &TEST_ENCRYPTION_KEY)
            .expect("update b should work");

        let b_after_update = store
            .get(&b_id)
            .expect("Not to error getting b")
            .expect("b to exist");

        assert_logins_equiv(&b2, &b_after_update);
        assert_ge!(b_after_update.record.time_created, start_us);
        assert_le!(b_after_update.record.time_created, now_us);
        assert_ge!(b_after_update.record.time_password_changed, now_us);
        assert_ge!(b_after_update.record.time_last_used, now_us);
        // Should be two even though we updated twice
        assert_eq!(b_after_update.record.times_used, 2);
    }

    #[test]
    fn test_sync_manager_registration() {
        let store = Arc::new(LoginStore::new_in_memory().unwrap());
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 0);
        Arc::clone(&store).register_with_sync_manager();
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        let registered = STORE_FOR_MANAGER.lock().upgrade().expect("should upgrade");
        assert!(Arc::ptr_eq(&store, &registered));
        drop(registered);
        // should be no new references
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        // dropping the registered object should drop the registration.
        drop(store);
        assert!(STORE_FOR_MANAGER.lock().upgrade().is_none());
    }
}

#[test]
fn test_send() {
    fn ensure_send<T: Send>() {}
    ensure_send::<LoginStore>();
}
