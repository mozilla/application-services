/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::models::address::{Address, UpdatableAddressFields};
use crate::db::models::credit_card::{CreditCard, UpdatableCreditCardFields};
use crate::db::{addresses, credit_cards, AutofillDb};
use crate::error::*;
use rusqlite::{
    types::{FromSql, ToSql},
    Connection,
};
use sql_support::{self, ConnExt};
use std::path::Path;
use std::sync::{Arc, Mutex};
use sync15_traits::SyncEngine;
use sync_guid::Guid;

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff - needed
    //        to wrap the Option<Store> as they aren't `Sync`
    static ref STORE_FOR_MANAGER: Mutex<Option<Store>> = Mutex::new(None);
}

// Get a Store for the SyncManager to use
pub fn get_store_for_manager() -> Option<Store> {
    STORE_FOR_MANAGER.lock().unwrap().as_ref().cloned()
}

// This is the type that uniffi exposes. It has `Arc<>` around the db mutex because
// register_with_sync_manager() needs to clone ourself.  This is redundant though, because uniffi
// also has an Arc<>.  One day https://github.com/mozilla/uniffi-rs/issues/419 will give us access
// to the `Arc<>` uniffi owns, and we can drop the extra Arc<> here.
#[derive(Clone)]
pub struct Store {
    // pub(crate) because db is used by the sync code
    pub(crate) db: Arc<Mutex<AutofillDb>>,
}

impl Store {
    fn with_db(db: AutofillDb) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
        }
    }

    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::with_db(AutofillDb::new(db_path)?))
    }

    #[cfg(test)]
    pub fn new_memory() -> Self {
        Self::with_db(crate::db::test::new_mem_db())
    }

    pub fn new_shared_memory(db_name: &str) -> Result<Self> {
        Ok(Self::with_db(AutofillDb::new_memory(db_name)?))
    }

    pub fn add_credit_card(&self, fields: UpdatableCreditCardFields) -> Result<CreditCard> {
        let credit_card = credit_cards::add_credit_card(&self.db.lock().unwrap().writer, fields)?;
        Ok(credit_card.into())
    }

    pub fn get_credit_card(&self, guid: String) -> Result<CreditCard> {
        let credit_card =
            credit_cards::get_credit_card(&self.db.lock().unwrap().writer, &Guid::new(&guid))?;
        Ok(credit_card.into())
    }

    pub fn get_all_credit_cards(&self) -> Result<Vec<CreditCard>> {
        let credit_cards = credit_cards::get_all_credit_cards(&self.db.lock().unwrap().writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(credit_cards)
    }

    pub fn update_credit_card(
        &self,
        guid: String,
        credit_card: UpdatableCreditCardFields,
    ) -> Result<()> {
        credit_cards::update_credit_card(
            &self.db.lock().unwrap().writer,
            &Guid::new(&guid),
            &credit_card,
        )
    }

    pub fn delete_credit_card(&self, guid: String) -> Result<bool> {
        credit_cards::delete_credit_card(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    pub fn touch_credit_card(&self, guid: String) -> Result<()> {
        credit_cards::touch(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    pub fn scrub_encrypted_data(&self) -> Result<()> {
        // Currently only credit cards have encrypted data
        credit_cards::scrub_encrypted_credit_card_data(&self.db.lock().unwrap().writer)?;
        // Force the sync engine to refetch data (only need to do this for the credit cards, since the
        // addresses engine doesn't store encrypted data).
        crate::sync::credit_card::create_engine(self.clone()).reset_local_sync_data()?;
        Ok(())
    }

    pub fn add_address(&self, new_address: UpdatableAddressFields) -> Result<Address> {
        Ok(addresses::add_address(&self.db.lock().unwrap().writer, new_address)?.into())
    }

    pub fn get_address(&self, guid: String) -> Result<Address> {
        Ok(addresses::get_address(&self.db.lock().unwrap().writer, &Guid::new(&guid))?.into())
    }

    pub fn get_all_addresses(&self) -> Result<Vec<Address>> {
        let addresses = addresses::get_all_addresses(&self.db.lock().unwrap().writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(addresses)
    }

    pub fn update_address(&self, guid: String, address: UpdatableAddressFields) -> Result<()> {
        addresses::update_address(&self.db.lock().unwrap().writer, &Guid::new(&guid), &address)
    }

    pub fn delete_address(&self, guid: String) -> Result<bool> {
        addresses::delete_address(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    pub fn touch_address(&self, guid: String) -> Result<()> {
        addresses::touch(&self.db.lock().unwrap().writer, &Guid::new(&guid))
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    //
    // Other components use a SyncManager method (for example `set_places()`).  The advantage of
    // this system is the consumer doesn't need a reference to the sync manager.
    pub fn register_with_sync_manager(&self) {
        STORE_FOR_MANAGER.lock().unwrap().replace(self.clone());
    }

    pub fn create_credit_cards_sync_engine(&self) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::credit_card::create_engine(self.clone()))
    }

    pub fn create_addresses_sync_engine(&self) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::address::create_engine(self.clone()))
    }
}

pub(crate) fn put_meta(conn: &Connection, key: &str, value: &dyn ToSql) -> Result<()> {
    conn.execute_named_cached(
        "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
        &[(":key", &key), (":value", value)],
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
    conn.execute_named_cached("DELETE FROM moz_meta WHERE key = :key", &[(":key", &key)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;
    use rusqlite::NO_PARAMS;

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

        db.writer.execute("DELETE FROM moz_meta", NO_PARAMS)?;

        Ok(())
    }

    #[test]
    fn test_store_for_manager() {
        let store = Store::new_memory();
        store.register_with_sync_manager();
        let store_for_manager = get_store_for_manager().unwrap();

        assert!(Arc::ptr_eq(&store_for_manager.db, &store.db));
        // To make sure the pointer check is correct, let's make sure it fails for a new store
        assert!(!Arc::ptr_eq(&store_for_manager.db, &Store::new_memory().db));

        // Check reference counting:
        //   - One reference in store
        //   - One reference in store_for_manager
        //   - One reference in store_impl
        assert_eq!(Arc::strong_count(&store_for_manager.db), 3);
    }
}
