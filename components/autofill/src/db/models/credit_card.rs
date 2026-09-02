/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Credit-card models and the cleartext fields that are stored encrypted.
//!
//! Only the number is encrypted today, and the encrypted value is that number
//! verbatim - nothing about the stored format changes here.
//!
//! `SecureCreditCardFields` is a struct because a CVV is expected as a second
//! encrypted field. Two values need a structured encoding, and existing rows
//! then have to be rewritten: they decrypt to a bare number, where the new code
//! would expect a structure. That migration is a separate ticket.

use super::Metadata;
use crate::encryption::{decrypt_str, encrypt_str, EncryptorDecryptor};
use crate::error::Error;
use rusqlite::Row;
use sync_guid::Guid;

#[derive(Debug, Clone, Default)]
pub struct UpdatableCreditCardFields {
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreditCard {
    pub guid: String,
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,

    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,

    // The metadata
    pub time_created: i64,
    pub time_last_used: Option<i64>,
    pub time_last_modified: i64,
    pub times_used: i64,
}

// This is used to "externalize" a credit-card, suitable for handing back to
// consumers.
impl From<InternalCreditCard> for CreditCard {
    fn from(icc: InternalCreditCard) -> Self {
        CreditCard {
            guid: icc.guid.to_string(),
            cc_name: icc.cc_name,
            cc_number_enc: icc.cc_number_enc,
            cc_number_last_4: icc.cc_number_last_4,
            cc_exp_month: icc.cc_exp_month,
            cc_exp_year: icc.cc_exp_year,
            cc_type: icc.cc_type,
            // note we can't use u64 in uniffi
            time_created: u64::from(icc.metadata.time_created) as i64,
            time_last_used: if icc.metadata.time_last_used.0 == 0 {
                None
            } else {
                Some(icc.metadata.time_last_used.0 as i64)
            },
            time_last_modified: u64::from(icc.metadata.time_last_modified) as i64,
            times_used: icc.metadata.times_used,
        }
    }
}

// NOTE: No `PartialEq` here because the same card number will encrypt to a
// different value each time it is encrypted, making it meaningless to compare.
#[derive(Debug, Clone, Default)]
pub struct InternalCreditCard {
    pub guid: Guid,
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
    pub metadata: Metadata,
}

impl InternalCreditCard {
    pub fn from_row(row: &Row<'_>) -> Result<InternalCreditCard, rusqlite::Error> {
        Ok(Self {
            guid: Guid::from_string(row.get("guid")?),
            cc_name: row.get("cc_name")?,
            cc_number_enc: row.get("cc_number_enc")?,
            cc_number_last_4: row.get("cc_number_last_4")?,
            cc_exp_month: row.get("cc_exp_month")?,
            cc_exp_year: row.get("cc_exp_year")?,
            cc_type: row.get("cc_type")?,
            metadata: Metadata {
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    }

    pub fn has_scrubbed_data(&self) -> bool {
        self.cc_number_enc.is_empty()
    }
}

/// Cleartext credit-card fields that are encrypted for local storage.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct SecureCreditCardFields {
    pub cc_number: String,
}

impl SecureCreditCardFields {
    /// `guid` only identifies the record in error messages.
    pub fn encrypt(
        &self,
        encdec: &dyn EncryptorDecryptor,
        guid: &str,
    ) -> crate::error::Result<String> {
        encrypt_str(encdec, &self.cc_number)
            .map_err(|e| Error::EncryptionFailed(format!("{e} (encrypting {guid})")))
    }

    pub fn decrypt(
        ciphertext: &str,
        encdec: &dyn EncryptorDecryptor,
        guid: &str,
    ) -> crate::error::Result<Self> {
        let cc_number = decrypt_str(encdec, ciphertext).map_err(|e| {
            Error::DecryptionFailed(format!(
                "{e} (decrypting {guid}, ciphertext length: {})",
                ciphertext.len()
            ))
        })?;
        Ok(Self { cc_number })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::{random_key_encryptor, ManagedEncryptorDecryptor};
    use nss_as::ensure_initialized;

    fn encdec() -> ManagedEncryptorDecryptor {
        ensure_initialized();
        random_key_encryptor().unwrap()
    }

    #[test]
    fn test_roundtrip() {
        let encdec = encdec();
        let stored = SecureCreditCardFields {
            cc_number: "4111111111117629".to_string(),
        }
        .encrypt(&encdec, "test-guid")
        .unwrap();

        assert!(!stored.is_empty());
        assert_ne!(
            stored, "4111111111117629",
            "the stored value must not be the cleartext"
        );
        assert_eq!(
            SecureCreditCardFields::decrypt(&stored, &encdec, "test-guid")
                .unwrap()
                .cc_number,
            "4111111111117629"
        );
    }

    #[test]
    fn test_decrypt_with_the_wrong_key_fails() {
        let stored = SecureCreditCardFields {
            cc_number: "4111111111117629".to_string(),
        }
        .encrypt(&encdec(), "test-guid")
        .unwrap();
        assert!(SecureCreditCardFields::decrypt(&stored, &encdec(), "test-guid").is_err());
    }

    #[test]
    fn test_scrubbed_is_the_default() {
        // Empty ciphertext marks data to be replaced from Sync.
        assert!(InternalCreditCard::default().has_scrubbed_data());
    }

    #[test]
    fn test_encrypting_twice_gives_different_ciphertext() {
        let encdec = encdec();
        let fields = SecureCreditCardFields {
            cc_number: "4111111111117629".to_string(),
        };
        assert_ne!(
            fields.encrypt(&encdec, "test-guid").unwrap(),
            fields.encrypt(&encdec, "test-guid").unwrap()
        );
    }
}
