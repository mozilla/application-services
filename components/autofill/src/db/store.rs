/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::models::address::{Address, ExternalizeAddress, NewAddressFields};
use crate::db::models::credit_card::{CreditCard, ExternalizeCreditCard, NewCreditCardFields};
use crate::db::{addresses, credit_cards, AutofillDb};
use crate::error::*;

use std::path::Path;

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
    pub fn add_credit_card(
        &self,
        new_credit_card_fields: NewCreditCardFields,
    ) -> Result<CreditCard> {
        let credit_card = credit_cards::add_credit_card(&self.db.writer, new_credit_card_fields)?;
        Ok(credit_card.to_external())
    }

    #[allow(dead_code)]
    pub fn get_credit_card(&self, guid: String) -> Result<CreditCard> {
        let credit_card = credit_cards::get_credit_card(&self.db.writer, guid)?;
        Ok(credit_card.to_external())
    }

    #[allow(dead_code)]
    pub fn get_all_credit_cards(&self) -> Result<Vec<CreditCard>> {
        let credit_cards = credit_cards::get_all_credit_cards(&self.db.writer)?
            .iter()
            .map(|x| x.to_external())
            .collect();
        Ok(credit_cards)
    }

    #[allow(dead_code)]
    pub fn update_credit_card(&self, credit_card: CreditCard) -> Result<()> {
        credit_cards::update_credit_card(&self.db.writer, &credit_card)
    }

    #[allow(dead_code)]
    pub fn delete_credit_card(&self, guid: String) -> Result<bool> {
        credit_cards::delete_credit_card(&self.db.writer, guid)
    }

    pub fn touch_credit_card(&self, guid: String) -> Result<()> {
        credit_cards::touch(&self.db.writer, guid)
    }

    #[allow(dead_code)]
    pub fn add_address(&self, new_address: NewAddressFields) -> Result<Address> {
        let address = addresses::add_address(&self.db.writer, new_address)?;
        Ok(address.to_external())
    }

    #[allow(dead_code)]
    pub fn get_address(&self, guid: String) -> Result<Address> {
        let address = addresses::get_address(&self.db.writer, guid)?;
        Ok(address.to_external())
    }

    #[allow(dead_code)]
    pub fn get_all_addresses(&self) -> Result<Vec<Address>> {
        let addresses = addresses::get_all_addresses(&self.db.writer)?
            .iter()
            .map(|x| x.to_external())
            .collect();
        Ok(addresses)
    }

    #[allow(dead_code)]
    pub fn update_address(&self, address: Address) -> Result<()> {
        addresses::update_address(&self.db.writer, &address)
    }

    #[allow(dead_code)]
    pub fn delete_address(&self, guid: String) -> Result<bool> {
        addresses::delete_address(&self.db.writer, guid)
    }

    #[allow(dead_code)]
    pub fn touch_address(&self, guid: String) -> Result<()> {
        addresses::touch(&self.db.writer, guid)
    }
}
