/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Rewrites credit-card rows that predate the versioned secure-fields blob.

use crate::db::store::{get_meta, put_meta};
use crate::db::AutofillDb;
use crate::encryption::{decrypt_str, encrypt_str, EncryptorDecryptor};
use crate::error::Result;
use error_support::info;
use rusqlite::{named_params, Transaction};
use serde::{Deserialize, Serialize};

const MIGRATION_DONE_META_KEY: &str = "cc_secure_fields_migrated";

/// Frozen on purpose: a later change to `SecureCreditCardFields` must not alter
/// what this migration already wrote for rows it has rewritten.
#[derive(Serialize)]
struct FrozenV1<'a> {
    v: u8,
    n: &'a str,
}

/// Only the presence of `v` matters, so the payload is irrelevant.
#[derive(Deserialize)]
struct VersionProbe {
    #[allow(dead_code)]
    v: u8,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CreditCardMigrationMetrics {
    pub migrated: u64,
    pub already_migrated: u64,
    pub undecryptable: u64,
    pub conflicted: u64,
}

/// The flag is only set when nothing was skipped: a row the key could not read
/// is left for a later run, because the key can be wrong now and right later.
pub(crate) fn migrate_cc_secure_fields_if_needed(
    db: &AutofillDb,
) -> Result<CreditCardMigrationMetrics> {
    if get_meta::<bool>(&db.writer, MIGRATION_DONE_META_KEY)?.unwrap_or(false) {
        return Ok(CreditCardMigrationMetrics::default());
    }

    let tx = db.writer.unchecked_transaction()?;
    let metrics = migrate_cc_secure_fields(&tx, db.encdec.as_ref())?;
    if metrics.undecryptable == 0 && metrics.conflicted == 0 {
        put_meta(&tx, MIGRATION_DONE_META_KEY, &true)?;
    }
    tx.commit()?;

    if metrics != CreditCardMigrationMetrics::default() {
        // No guids and no card data - just counts, so this is safe to log.
        info!(
            "cc secure-fields migration: {} migrated, {} already migrated, \
             {} unreadable, {} conflicted",
            metrics.migrated, metrics.already_migrated, metrics.undecryptable, metrics.conflicted
        );
    }
    Ok(metrics)
}

pub(crate) fn migrate_cc_secure_fields(
    tx: &Transaction<'_>,
    encdec: &dyn EncryptorDecryptor,
) -> Result<CreditCardMigrationMetrics> {
    let mut metrics = CreditCardMigrationMetrics::default();

    // Empty ciphertext marks scrubbed data waiting to be replaced from Sync,
    // not an encrypted number, so it has to stay empty.
    let rows = tx
        .prepare("SELECT guid, cc_number_enc FROM credit_cards_data WHERE cc_number_enc != ''")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (guid, old_ciphertext) in rows {
        // Giving up on a card is what
        // `scrub_undecryptable_credit_card_data_for_remote_replacement` is for.
        let Ok(cleartext) = decrypt_str(encdec, &old_ciphertext) else {
            metrics.undecryptable += 1;
            continue;
        };

        if serde_json::from_str::<VersionProbe>(&cleartext).is_ok() {
            metrics.already_migrated += 1;
            continue;
        }

        let blob = serde_json::to_string(&FrozenV1 {
            v: 1,
            n: &cleartext,
        })?;
        let new_ciphertext = encrypt_str(encdec, &blob)?;

        // Matching the old ciphertext makes this a compare-and-swap, so a row
        // written between the SELECT above and here is not clobbered. No other
        // column is named, which is the point of doing this in raw SQL.
        let updated = tx.execute(
            "UPDATE credit_cards_data
                SET cc_number_enc = :new
              WHERE guid = :guid AND cc_number_enc = :old",
            named_params! {
                ":new": &new_ciphertext,
                ":guid": &guid,
                ":old": &old_ciphertext,
            },
        )?;

        if updated == 1 {
            metrics.migrated += 1;
        } else {
            metrics.conflicted += 1;
        }
    }

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::credit_cards::{add_credit_card, get_credit_card};
    use crate::db::models::credit_card::{
        InternalCreditCard, SecureCreditCardFields, UpdatableCreditCardFields,
    };
    use crate::db::test::{new_mem_db, new_mem_db_with_encdec};
    use crate::encryption::{static_key_encryptor, ManagedEncryptorDecryptor};
    use nss_as::ensure_initialized;
    use std::sync::Arc;
    use sync_guid::Guid;

    const NUMBER: &str = "4111111111117629";

    fn add_legacy_card(
        db: &AutofillDb,
        encdec: &dyn EncryptorDecryptor,
        number: &str,
    ) -> InternalCreditCard {
        add_credit_card(
            db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number_enc: encrypt_str(encdec, number).unwrap(),
                cc_number_last_4: number[number.len() - 4..].to_string(),
                cc_exp_month: 9,
                cc_exp_year: 2027,
                cc_type: "visa".to_string(),
            },
        )
        .unwrap()
    }

    fn run(db: &AutofillDb) -> CreditCardMigrationMetrics {
        let tx = db.writer.unchecked_transaction().unwrap();
        let metrics = migrate_cc_secure_fields(&tx, db.encdec.as_ref()).unwrap();
        tx.commit().unwrap();
        metrics
    }

    fn run_if_needed(db: &AutofillDb) -> CreditCardMigrationMetrics {
        migrate_cc_secure_fields_if_needed(db).unwrap()
    }

    fn new_encdec() -> ManagedEncryptorDecryptor {
        ensure_initialized();
        static_key_encryptor(&db_crypto::create_key().unwrap()).unwrap()
    }

    #[test]
    fn test_legacy_row_is_rewritten_as_a_blob() {
        let db = new_mem_db();
        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);

        let metrics = run(&db);
        assert_eq!(metrics.migrated, 1);
        assert_eq!(metrics.already_migrated, 0);

        let stored = get_credit_card(&db, &card.guid).unwrap().cc_number_enc;
        assert_ne!(
            stored, card.cc_number_enc,
            "the ciphertext must have been replaced"
        );
        assert_eq!(
            decrypt_str(db.encdec.as_ref(), &stored).unwrap(),
            format!(r#"{{"v":1,"n":"{NUMBER}"}}"#),
            "the on-disk format is frozen - a change here breaks v1 readers"
        );
    }

    #[test]
    fn test_the_number_survives_the_rewrite() {
        let db = new_mem_db();
        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);
        run(&db);

        let stored = get_credit_card(&db, &card.guid).unwrap().cc_number_enc;
        assert_eq!(
            SecureCreditCardFields::decrypt(&stored, db.encdec.as_ref(), card.guid.as_str())
                .unwrap()
                .cc_number,
            NUMBER
        );
    }

    #[test]
    fn test_sync_metadata_is_not_touched() {
        let db = new_mem_db();
        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);
        let before = get_credit_card(&db, &card.guid).unwrap().metadata;

        run(&db);

        let after = get_credit_card(&db, &card.guid).unwrap().metadata;
        assert_eq!(
            after.sync_change_counter, before.sync_change_counter,
            "a re-encryption is not a user edit - bumping this uploads every card"
        );
        assert_eq!(after.time_last_modified, before.time_last_modified);
        assert_eq!(after.time_created, before.time_created);
        assert_eq!(after.time_last_used, before.time_last_used);
        assert_eq!(after.times_used, before.times_used);
    }

    #[test]
    fn test_second_run_is_a_noop() {
        let db = new_mem_db();
        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);

        assert_eq!(run(&db).migrated, 1);
        let after_first = get_credit_card(&db, &card.guid).unwrap().cc_number_enc;

        let metrics = run(&db);
        assert_eq!(metrics.migrated, 0);
        assert_eq!(metrics.already_migrated, 1);
        assert_eq!(
            get_credit_card(&db, &card.guid).unwrap().cc_number_enc,
            after_first,
            "a row already in the new format must not be re-encrypted"
        );
    }

    #[test]
    fn test_undecryptable_row_is_left_alone() {
        let db = new_mem_db_with_encdec(Arc::new(new_encdec()));
        let old_encdec = new_encdec();
        let card = add_legacy_card(&db, &old_encdec, "2345678923456789");

        let metrics = run(&db);
        assert_eq!(metrics.undecryptable, 1);
        assert_eq!(metrics.migrated, 0);
        assert_eq!(
            get_credit_card(&db, &card.guid).unwrap().cc_number_enc,
            card.cc_number_enc,
            "an unreadable row must survive untouched, not be scrubbed"
        );
    }

    #[test]
    fn test_scrubbed_row_stays_scrubbed() {
        let db = new_mem_db();
        let card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number_enc: String::new(),
                cc_number_last_4: "7629".to_string(),
                cc_exp_month: 9,
                cc_exp_year: 2027,
                cc_type: "visa".to_string(),
            },
        )
        .unwrap();

        let metrics = run(&db);
        assert_eq!(metrics, CreditCardMigrationMetrics::default());
        assert!(
            get_credit_card(&db, &card.guid)
                .unwrap()
                .cc_number_enc
                .is_empty(),
            "empty ciphertext means 'replace from Sync', not 'encrypted'"
        );
    }

    #[test]
    fn test_mixed_formats_in_one_pass() {
        let db = new_mem_db();
        let legacy = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);
        let already = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number_enc: SecureCreditCardFields {
                    cc_number: "5500005555555559".to_string(),
                }
                .encrypt(db.encdec.as_ref(), "new-row")
                .unwrap(),
                cc_number_last_4: "5559".to_string(),
                cc_exp_month: 1,
                cc_exp_year: 2030,
                cc_type: "mastercard".to_string(),
            },
        )
        .unwrap();

        let metrics = run(&db);
        assert_eq!(metrics.migrated, 1);
        assert_eq!(metrics.already_migrated, 1);

        for (guid, expected) in [(&legacy.guid, NUMBER), (&already.guid, "5500005555555559")] {
            let stored = get_credit_card(&db, guid).unwrap().cc_number_enc;
            assert_eq!(
                SecureCreditCardFields::decrypt(&stored, db.encdec.as_ref(), guid.as_str())
                    .unwrap()
                    .cc_number,
                expected
            );
        }
    }

    #[test]
    fn test_empty_table_is_fine() {
        let db = new_mem_db();
        let metrics = run(&db);
        assert_eq!(metrics, CreditCardMigrationMetrics::default());
        let _ = Guid::new("unused");
    }

    #[test]
    fn test_the_flag_stops_the_second_pass() {
        let db = new_mem_db();
        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);

        assert_eq!(run_if_needed(&db).migrated, 1);
        let after_first = get_credit_card(&db, &card.guid).unwrap().cc_number_enc;

        let metrics = run_if_needed(&db);
        assert_eq!(
            metrics,
            CreditCardMigrationMetrics::default(),
            "a finished migration must not look at the rows again - not even to \
             count them as already migrated"
        );
        assert_eq!(
            get_credit_card(&db, &card.guid).unwrap().cc_number_enc,
            after_first
        );
    }

    #[test]
    fn test_the_flag_is_withheld_while_a_row_is_unreadable() {
        let db = new_mem_db_with_encdec(Arc::new(new_encdec()));
        let old_encdec = new_encdec();
        add_legacy_card(&db, &old_encdec, "2345678923456789");
        add_legacy_card(&db, db.encdec.as_ref(), NUMBER);

        let first = run_if_needed(&db);
        assert_eq!(first.migrated, 1);
        assert_eq!(first.undecryptable, 1);

        let second = run_if_needed(&db);
        assert_eq!(second.undecryptable, 1, "the pass was wrongly marked done");
        assert_eq!(second.already_migrated, 1);
        assert_eq!(second.migrated, 0);
    }

    #[test]
    fn test_an_empty_table_still_marks_the_migration_done() {
        let db = new_mem_db();
        assert_eq!(run_if_needed(&db), CreditCardMigrationMetrics::default());

        let card = add_legacy_card(&db, db.encdec.as_ref(), NUMBER);
        let metrics = run_if_needed(&db);
        assert_eq!(metrics, CreditCardMigrationMetrics::default());
        assert_eq!(
            get_credit_card(&db, &card.guid).unwrap().cc_number_enc,
            card.cc_number_enc,
            "the flag is set, so this legacy row is not picked up - which is why \
             the format switch has to ship with the migration, not after it"
        );
    }
}
