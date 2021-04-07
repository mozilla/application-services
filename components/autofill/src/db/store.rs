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
use std::cell::RefCell;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use sync15_traits::SyncEngine;
use sync_guid::Guid;

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff - needed
    //        to wrap the RefCell as they aren't `Sync`
    // RefCell: So we can replace what it holds. Normally you'd use `get_ref()`
    //          on the mutex and avoid the RefCell entirely, but that requires
    //          the mutex to be declared as `mut` which is apparently
    //          impossible in a `lazy_static`
    // [Arc/Weak]<StoreImpl>: What the sync manager actually needs.
    pub static ref STORE_FOR_MANAGER: Mutex<RefCell<Weak<StoreImpl>>> = Mutex::new(RefCell::new(Weak::new()));
}

// This is the type that uniffi exposes. It holds an `Arc<>` around the
// actual implementation, because we need to hand a clone of this `Arc<>` to
// the sync manager and to sync engines. One day
// https://github.com/mozilla/uniffi-rs/issues/417 will give us access to the
// `Arc<>` uniffi owns, which means we can drop this entirely (ie, `Store` and
// `StoreImpl` could be re-unified)
// Sadly, this is `pub` because our `autofill-utils` example uses it.
pub struct Store {
    store_impl: Arc<StoreImpl>,
}

impl Store {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            store_impl: Arc::new(StoreImpl::new(db_path)?),
        })
    }

    pub fn new_shared_memory(db_name: &str) -> Result<Self> {
        Ok(Self {
            store_impl: Arc::new(StoreImpl::new_shared_memory(db_name)?),
        })
    }

    pub fn add_credit_card(&self, fields: UpdatableCreditCardFields) -> Result<CreditCard> {
        self.store_impl.add_credit_card(fields)
    }

    pub fn get_credit_card(&self, guid: String) -> Result<CreditCard> {
        self.store_impl.get_credit_card(guid)
    }

    pub fn get_all_credit_cards(&self) -> Result<Vec<CreditCard>> {
        self.store_impl.get_all_credit_cards()
    }

    pub fn update_credit_card(
        &self,
        guid: String,
        credit_card: UpdatableCreditCardFields,
    ) -> Result<()> {
        self.store_impl.update_credit_card(guid, credit_card)
    }

    pub fn delete_credit_card(&self, guid: String) -> Result<bool> {
        self.store_impl.delete_credit_card(guid)
    }

    pub fn touch_credit_card(&self, guid: String) -> Result<()> {
        self.store_impl.touch_credit_card(guid)
    }

    pub fn add_address(&self, new_address: UpdatableAddressFields) -> Result<Address> {
        self.store_impl.add_address(new_address)
    }

    pub fn get_address(&self, guid: String) -> Result<Address> {
        self.store_impl.get_address(guid)
    }

    pub fn get_all_addresses(&self) -> Result<Vec<Address>> {
        self.store_impl.get_all_addresses()
    }

    pub fn update_address(&self, guid: String, address: UpdatableAddressFields) -> Result<()> {
        self.store_impl.update_address(guid, address)
    }

    pub fn delete_address(&self, guid: String) -> Result<bool> {
        self.store_impl.delete_address(guid)
    }

    pub fn touch_address(&self, guid: String) -> Result<()> {
        self.store_impl.touch_address(guid)
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    pub fn register_with_sync_manager(&self) {
        STORE_FOR_MANAGER
            .lock()
            .unwrap()
            .replace(Arc::downgrade(&self.store_impl));
    }

    // These 2 are odd ones out - they don't just delegate but instead
    // hand off the Arc.
    // Currently the only consumer of this is our "example" (and hence why they
    // are `pub` and not `pub(crate)`) - the sync manager duplicates it (because
    // it doesn't have a reference to us, just to the store_impl)
    // We could probably make the example work with the sync manager - but then
    // our example would link with places and logins etc.
    pub fn create_credit_cards_sync_engine(&self) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::credit_card::create_engine(
            self.store_impl.clone(),
        ))
    }

    pub fn create_addresses_sync_engine(&self) -> Box<dyn SyncEngine> {
        Box::new(crate::sync::address::create_engine(self.store_impl.clone()))
    }
}

// This is the actual implementation. All code in this crate works with this.
// Sadly, it's forced to be `pub` because the SyncManager also uses it.
pub struct StoreImpl {
    pub(crate) db: Mutex<AutofillDb>,
}

impl StoreImpl {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
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

    // Creates a store backed by an in-memory database that shares its memory API (required for autofill sync tests).
    pub fn new_shared_memory(db_name: &str) -> Result<Self> {
        Ok(Self {
            db: Mutex::new(AutofillDb::new_memory(db_name)?),
        })
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
}
