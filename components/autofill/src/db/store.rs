/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::models::address::{Address, UpdatableAddressFields};
use crate::db::models::credit_card::{CreditCard, UpdatableCreditCardFields};
use crate::db::{addresses, credit_cards, AutofillDb};
use crate::error::*;
use std::path::Path;
use sync_guid::Guid;

#[allow(dead_code)]
pub struct Store {
    db: AutofillDb,
}

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

    #[allow(dead_code)]
    pub fn add_credit_card(&self, fields: UpdatableCreditCardFields) -> Result<CreditCard> {
        let credit_card = credit_cards::add_credit_card(&self.db.writer, fields)?;
        Ok(credit_card.into())
    }

    #[allow(dead_code)]
    pub fn get_credit_card(&self, guid: String) -> Result<CreditCard> {
        let credit_card = credit_cards::get_credit_card(&self.db.writer, &Guid::new(&guid))?;
        Ok(credit_card.into())
    }

    #[allow(dead_code)]
    pub fn get_all_credit_cards(&self) -> Result<Vec<CreditCard>> {
        let credit_cards = credit_cards::get_all_credit_cards(&self.db.writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(credit_cards)
    }

    #[allow(dead_code)]
    pub fn update_credit_card(
        &self,
        guid: String,
        credit_card: UpdatableCreditCardFields,
    ) -> Result<()> {
        credit_cards::update_credit_card(&self.db.writer, &Guid::new(&guid), &credit_card)
    }

    #[allow(dead_code)]
    pub fn delete_credit_card(&self, guid: String) -> Result<bool> {
        credit_cards::delete_credit_card(&self.db.writer, &Guid::new(&guid))
    }

    pub fn touch_credit_card(&self, guid: String) -> Result<()> {
        credit_cards::touch(&self.db.writer, &Guid::new(&guid))
    }

    #[allow(dead_code)]
    pub fn add_address(&self, new_address: UpdatableAddressFields) -> Result<Address> {
        Ok(addresses::add_address(&self.db.writer, new_address)?.into())
    }

    #[allow(dead_code)]
    pub fn get_address(&self, guid: String) -> Result<Address> {
        Ok(addresses::get_address(&self.db.writer, &Guid::new(&guid))?.into())
    }

    #[allow(dead_code)]
    pub fn get_all_addresses(&self) -> Result<Vec<Address>> {
        let addresses = addresses::get_all_addresses(&self.db.writer)?
            .into_iter()
            .map(|x| x.into())
            .collect();
        Ok(addresses)
    }

    #[allow(dead_code)]
    pub fn update_address(&self, guid: String, address: UpdatableAddressFields) -> Result<()> {
        addresses::update_address(&self.db.writer, &Guid::new(&guid), &address)
    }

    #[allow(dead_code)]
    pub fn delete_address(&self, guid: String) -> Result<bool> {
        addresses::delete_address(&self.db.writer, &Guid::new(&guid))
    }

    #[allow(dead_code)]
    pub fn touch_address(&self, guid: String) -> Result<()> {
        addresses::touch(&self.db.writer, &Guid::new(&guid))
    }
}
