use crate::db::LoginDb;
use crate::decrypt_login;
use crate::error::*;
use crate::login::{Login, LoginEntry};
use crate::store::LoginStore;
use nss::ensure_initialized_with_profile_dir;
use nss::pk11::sym_key::retrieve_or_create_and_import_and_persist_aes256_key_data;
use std::path::Path;
use std::sync::Arc;
use sync15::engine::SyncEngine;

// nickname for encryption key stored in NSS
static KEY_NAME: &str = "as-logins-key";

/*
 * Try to retrieve key from NSS key storage. If there is one, return it.
 * Otherwise create a key, store and return it.
*/
fn get_key() -> String {
    let key = retrieve_or_create_and_import_and_persist_aes256_key_data(KEY_NAME).unwrap();
    serde_json::to_string(&jwcrypto::Jwk::new_direct_from_bytes(None, &key)).unwrap()
}

pub struct ManagedLoginStore {
    pub store: LoginStore,
    pub key: String,
}

impl ManagedLoginStore {
    pub fn new(path: impl AsRef<Path>) -> ApiResult<Self> {
        ensure_initialized_with_profile_dir(&path);

        let db_filename = path.as_ref().join("as-logins.db");
        match LoginStore::new(db_filename) {
            Ok(store) => {
                let key = get_key();
                Ok(Self { store, key })
            }
            Err(e) => Err(e),
        }
    }

    pub fn new_from_db(path: impl AsRef<Path>, db: LoginDb) -> Self {
        ensure_initialized_with_profile_dir(&path);

        let store = LoginStore::new_from_db(db);
        let key = get_key();
        Self { store, key }
    }

    pub fn new_in_memory(path: impl AsRef<Path>) -> ApiResult<Self> {
        ensure_initialized_with_profile_dir(&path);

        match LoginStore::new_in_memory() {
            Ok(store) => {
                let key = get_key();
                Ok(Self { store, key })
            }
            Err(e) => Err(e),
        }
    }

    pub fn list(&self) -> ApiResult<Vec<Login>> {
        match self.store.list() {
            Ok(encrypted_logins) => Ok(encrypted_logins
                .into_iter()
                .map(|encrypted_login| decrypt_login(encrypted_login, &self.key).unwrap())
                .collect()),
            Err(e) => Err(e),
        }
    }

    pub fn get(&self, id: &str) -> ApiResult<Option<Login>> {
        match self.store.get(id) {
            Ok(Some(encrypted_login)) => Ok(Some(decrypt_login(encrypted_login, &self.key)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_by_base_domain(&self, base_domain: &str) -> ApiResult<Vec<Login>> {
        match self.store.get_by_base_domain(base_domain) {
            Ok(encrypted_logins) => Ok(encrypted_logins
                .into_iter()
                .map(|encrypted_login| decrypt_login(encrypted_login, &self.key).unwrap())
                .collect()),
            Err(e) => Err(e),
        }
    }

    pub fn find_login_to_update(&self, entry: LoginEntry) -> ApiResult<Option<Login>> {
        self.store.find_login_to_update(entry, &self.key)
    }

    pub fn touch(&self, id: &str) -> ApiResult<()> {
        self.store.touch(id)
    }

    pub fn delete(&self, id: &str) -> ApiResult<bool> {
        self.store.delete(id)
    }

    pub fn wipe_local(&self) -> ApiResult<()> {
        self.store.wipe_local()
    }

    pub fn reset(self: Arc<Self>) -> ApiResult<()> {
        todo!()
    }

    pub fn update(&self, id: &str, entry: LoginEntry) -> ApiResult<Login> {
        match self.store.update(id, entry, &self.key) {
            Ok(encrypted_login) => Ok(decrypt_login(encrypted_login, &self.key)?),
            Err(e) => Err(e),
        }
    }

    pub fn add(&self, entry: LoginEntry) -> ApiResult<Login> {
        match self.store.add(entry, &self.key) {
            Ok(encrypted_login) => Ok(decrypt_login(encrypted_login, &self.key)?),
            Err(e) => Err(e),
        }
    }

    pub fn add_or_update(&self, entry: LoginEntry) -> ApiResult<Login> {
        match self.store.add_or_update(entry, &self.key) {
            Ok(encrypted_login) => Ok(decrypt_login(encrypted_login, &self.key)?),
            Err(e) => Err(e),
        }
    }

    pub fn register_with_sync_manager(self: Arc<Self>) {
        todo!()
    }

    pub fn create_logins_sync_engine(self: Arc<Self>) -> ApiResult<Box<dyn SyncEngine>> {
        todo!()
    }
}

#[cfg(test)]
mod test_managed_store {
    use super::*;
    use crate::util;
    use crate::{LoginFields, SecureLoginFields};
    use more_asserts::*;
    use std::cmp::Reverse;
    use std::env;
    use std::time::SystemTime;

    fn assert_logins_equiv(a: &LoginEntry, b: &Login) {
        assert_eq!(a.fields, b.fields);
        assert_eq!(b.sec_fields, a.sec_fields);
    }

    #[test]
    fn test_general() {
        let pathname = env::var("PROFILE_DIR").expect("missing PROFILE_DIR");
        let path = Path::new(&pathname);
        let store = ManagedLoginStore::new_in_memory(path).unwrap();
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
        let a_id = store.add(a.clone()).expect("added a").record.id;
        let b_id = store.add(b.clone()).expect("added b").record.id;

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
            .update(&b_id, b2.clone())
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
}

#[test]
fn test_send() {
    fn ensure_send<T: Send>() {}
    ensure_send::<ManagedLoginStore>();
}
