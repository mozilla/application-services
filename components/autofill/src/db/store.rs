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
use sync_guid::Guid;

#[allow(dead_code)]
pub struct Store {
    db: AutofillDb,
}

#[allow(dead_code)]
impl Store {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: AutofillDb::new(db_path)?,
        })
    }

    /// Creates a store backed by an in-memory database.
    #[cfg(test)]
    pub fn new_memory(db_path: &str) -> Result<Self> {
        Ok(Self {
            db: AutofillDb::new_memory(db_path)?,
        })
    }

    #[cfg(test)] // XXX - maybe this should just be `impl ConnExt`?
    pub fn db(&self) -> &AutofillDb {
        &self.db
    }

    pub fn add_credit_card(&self, fields: UpdatableCreditCardFields) -> Result<CreditCard> {
        let credit_card = credit_cards::add_credit_card(&self.db.writer, fields)?;
        Ok(credit_card.into())
    }

    pub fn get_credit_card(&self, guid: String) -> Result<CreditCard> {
        let credit_card = credit_cards::get_credit_card(&self.db.writer, &Guid::new(&guid))?;
        Ok(credit_card.into())
    }

    pub fn get_all_credit_cards(&self) -> Result<Vec<CreditCard>> {
        let credit_cards = credit_cards::get_all_credit_cards(&self.db.writer)?
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
        credit_cards::update_credit_card(&self.db.writer, &Guid::new(&guid), &credit_card)
    }

    pub fn delete_credit_card(&self, guid: String) -> Result<bool> {
        credit_cards::delete_credit_card(&self.db.writer, &Guid::new(&guid))
    }

    pub fn touch_credit_card(&self, guid: String) -> Result<()> {
        credit_cards::touch(&self.db.writer, &Guid::new(&guid))
    }

    pub fn add_address(&self, new_address: UpdatableAddressFields) -> Result<Address> {
        Ok(addresses::add_address(&self.db.writer, new_address)?.into())
    }

    pub fn get_address(&self, guid: String) -> Result<Address> {
        Ok(addresses::get_address(&self.db.writer, &Guid::new(&guid))?.into())
    }

    pub fn get_all_addresses(&self) -> Result<Vec<Address>> {
        let addresses = addresses::get_all_addresses(&self.db.writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(addresses)
    }

    pub fn update_address(&self, guid: String, address: UpdatableAddressFields) -> Result<()> {
        addresses::update_address(&self.db.writer, &Guid::new(&guid), &address)
    }

    pub fn delete_address(&self, guid: String) -> Result<bool> {
        addresses::delete_address(&self.db.writer, &Guid::new(&guid))
    }

    pub fn touch_address(&self, guid: String) -> Result<()> {
        addresses::touch(&self.db.writer, &Guid::new(&guid))
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
