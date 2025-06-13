/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::models::address::{Address, UpdatableAddressFields};
use crate::db::models::credit_card::{CreditCard, UpdatableCreditCardFields};
use crate::db::{addresses, credit_cards, AutofillDb};
use crate::error::*;
use error_support::handle_error;
use rusqlite::{
    types::{FromSql, ToSql},
    Connection,
};
use sql_support::{self, run_maintenance, ConnExt};
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use sync15::engine::{SyncEngine, SyncEngineId};
use sync_guid::Guid;

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the contents - needed to wrap
    //        the Weak as it isn't `Sync`
    // [Arc/Weak]<Store>: What the sync manager actually needs.
    static ref STORE_FOR_MANAGER: Mutex<Weak<Store>> = Mutex::new(Weak::new());
}

/// Called by the sync manager to get a sync engine via the store previously
/// registered with the sync manager.
pub fn get_registered_sync_engine(engine_id: &SyncEngineId) -> Option<Box<dyn SyncEngine>> {
    let weak = STORE_FOR_MANAGER.lock().unwrap();
    match weak.upgrade() {
        None => None,
        Some(store) => match engine_id {
            SyncEngineId::Addresses => Some(Box::new(crate::sync::address::create_engine(store))),
            SyncEngineId::CreditCards => {
                Some(Box::new(crate::sync::credit_card::create_engine(store)))
            }
            // panicking here seems reasonable - it's a static error if this
            // it hit, not something that runtime conditions can influence.
            _ => unreachable!("can't provide unknown engine: {}", engine_id),
        },
    }
}

// This is the type that uniffi exposes.
pub struct Store {
    pub(crate) db: Mutex<AutofillDb>,
}

impl Store {
    #[handle_error(Error)]
    pub fn new(db_path: impl AsRef<Path>) -> ApiResult<Self> {
        Ok(Self {
            db: Mutex::new(AutofillDb::new(db_path)?),
        })
    }

    /// Creates a store backed by an in-memory database with its own memory API (required for unit tests).
    #[cfg(test)]
    pub fn new_memory() -> Self {
        Self {
            db: Mutex::new(crate::db::test::new_mem_db()),
        }
    }

    /// Creates a store backed by an in-memory database that shares its memory API (required for autofill sync tests).
    #[handle_error(Error)]
    pub fn new_shared_memory(db_name: &str) -> ApiResult<Self> {
        Ok(Self {
            db: Mutex::new(AutofillDb::new_memory(db_name)?),
        })
    }

    #[handle_error(Error)]
    pub fn add_credit_card(&self, fields: UpdatableCreditCardFields) -> ApiResult<CreditCard> {
        let credit_card = credit_cards::add_credit_card(&self.db.lock().unwrap().writer, fields)?;
        Ok(credit_card.into())
    }

    #[handle_error(Error)]
    pub fn get_credit_card(&self, guid: String) -> ApiResult<CreditCard> {
        let credit_card =
            credit_cards::get_credit_card(&self.db.lock().unwrap().writer, &Guid::new(&guid))?;
        Ok(credit_card.into())
    }

    #[handle_error(Error)]
    pub fn get_all_credit_cards(&self) -> ApiResult<Vec<CreditCard>> {
        let credit_cards = credit_cards::get_all_credit_cards(&self.db.lock().unwrap().writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(credit_cards)
    }

    #[handle_error(Error)]
    pub fn update_credit_card(
        &self,
        guid: String,
        credit_card: UpdatableCreditCardFields,
    ) -> ApiResult<()> {
        credit_cards::update_credit_card(
            &self.db.lock().unwrap().writer,
            &Guid::new(&guid),
            &credit_card,
        )
    }

    #[handle_error(Error)]
    pub fn delete_credit_card(&self, guid: String) -> ApiResult<bool> {
        credit_cards::delete_credit_card(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    #[handle_error(Error)]
    pub fn touch_credit_card(&self, guid: String) -> ApiResult<()> {
        credit_cards::touch(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    #[handle_error(Error)]
    pub fn add_address(&self, new_address: UpdatableAddressFields) -> ApiResult<Address> {
        Ok(addresses::add_address(&self.db.lock().unwrap().writer, new_address)?.into())
    }

    #[handle_error(Error)]
    pub fn get_address(&self, guid: String) -> ApiResult<Address> {
        Ok(addresses::get_address(&self.db.lock().unwrap().writer, &Guid::new(&guid))?.into())
    }

    #[handle_error(Error)]
    pub fn get_all_addresses(&self) -> ApiResult<Vec<Address>> {
        let addresses = addresses::get_all_addresses(&self.db.lock().unwrap().writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(addresses)
    }

    #[handle_error(Error)]
    pub fn update_address(&self, guid: String, address: UpdatableAddressFields) -> ApiResult<()> {
        addresses::update_address(&self.db.lock().unwrap().writer, &Guid::new(&guid), &address)
    }

    #[handle_error(Error)]
    pub fn delete_address(&self, guid: String) -> ApiResult<bool> {
        addresses::delete_address(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    #[handle_error(Error)]
    pub fn touch_address(&self, guid: String) -> ApiResult<()> {
        addresses::touch(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    #[handle_error(Error)]
    pub fn scrub_encrypted_data(self: Arc<Self>) -> ApiResult<()> {
        // scrub the data on disk
        // Currently only credit cards have encrypted data
        credit_cards::scrub_encrypted_credit_card_data(&self.db.lock().unwrap().writer)?;
        // Force the sync engine to refetch data (only need to do this for the credit cards, since the
        // addresses engine doesn't store encrypted data).
        crate::sync::credit_card::create_engine(self).reset_local_sync_data()?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn run_maintenance(&self) -> ApiResult<()> {
        let conn = self.db.lock().unwrap();
        run_maintenance(&conn)?;
        Ok(())
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

    // These 2 are a little odd - they aren't exposed by uniffi - currently the
    // only consumer of this is our "example" (and hence why they
    // are `pub` and not `pub(crate)`).
    // We could probably make the example work with the sync manager - but then
    // our example would link with places and logins etc, and it's not a big
    // deal really.
    pub fn create_credit_cards_sync_engine(self: Arc<Self>) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::credit_card::create_engine(self))
    }

    pub fn create_addresses_sync_engine(self: Arc<Self>) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::address::create_engine(self))
    }
}

pub(crate) fn put_meta(conn: &Connection, key: &str, value: &dyn ToSql) -> Result<()> {
    conn.execute_cached(
        "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
        &[(":key", &key as &dyn ToSql), (":value", value)],
    )?;
    Ok(())
}

pub(crate) fn get_meta<T: FromSql>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let res = conn.try_query_one(
        "SELECT value FROM moz_meta WHERE key = :key",
        &[(":key", &key)],
        true,
    )?;
    Ok(res)
}

pub(crate) fn delete_meta(conn: &Connection, key: &str) -> Result<()> {
    conn.execute_cached("DELETE FROM moz_meta WHERE key = :key", &[(":key", &key)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    #[test]
    fn test_autofill_meta() -> Result<()> {
        let db = new_mem_db();
        let test_key = "TEST KEY A";
        let test_value = "TEST VALUE A";
        let test_key2 = "TEST KEY B";
        let test_value2 = "TEST VALUE B";

        put_meta(&db, test_key, &test_value)?;
        put_meta(&db, test_key2, &test_value2)?;

        let retrieved_value: String = get_meta(&db, test_key)?.expect("test value");
        let retrieved_value2: String = get_meta(&db, test_key2)?.expect("test value 2");

        assert_eq!(retrieved_value, test_value);
        assert_eq!(retrieved_value2, test_value2);

        // check that the value of an existing key can be updated
        let test_value3 = "TEST VALUE C";
        put_meta(&db, test_key, &test_value3)?;

        let retrieved_value3: String = get_meta(&db, test_key)?.expect("test value 3");

        assert_eq!(retrieved_value3, test_value3);

        // check that a deleted key is not retrieved
        delete_meta(&db, test_key)?;
        let retrieved_value4: Option<String> = get_meta(&db, test_key)?;
        assert!(retrieved_value4.is_none());

        db.writer.execute("DELETE FROM moz_meta", [])?;

        Ok(())
    }

    #[test]
    fn test_sync_manager_registration() {
        let store = Arc::new(Store::new_shared_memory("sync-mgr-test").unwrap());
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
