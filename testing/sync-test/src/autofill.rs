/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::auth::TestClient;
use crate::testing::TestGroup;
use anyhow::Result;
use autofill::{
    db::{
        models::address::{Address, UpdatableAddressFields},
        models::credit_card::{CreditCard, UpdatableCreditCardFields},
        store::Store as AutofillStore,
    },
    encryption::{create_autofill_key, encrypt_string},
    error::ApiResult as AutofillResult,
};
use std::{collections::HashMap, sync::Arc};

pub fn sync_addresses(client: &mut TestClient) -> Result<()> {
    client.sync(&["addresses".to_string()], HashMap::new())?;
    Ok(())
}

pub fn add_address(s: &AutofillStore, a: UpdatableAddressFields) -> AutofillResult<Address> {
    let id = s.add_address(a)?.guid;
    Ok(s.get_address(id).expect("Address has been added"))
}

pub fn delete_address(s: &AutofillStore, a: Address) -> AutofillResult<()> {
    s.delete_address(a.guid)?;
    Ok(())
}

pub fn verify_address(s: &AutofillStore, a: &Address) {
    let equivalent = s
        .get_address(a.guid.clone())
        .expect("get_address() to succeed");
    assert_address_equiv(&equivalent, a);
}

pub fn verify_address_removal(s: &AutofillStore) {
    let a = s.get_all_addresses().expect("no returned addresses");
    assert!(a.is_empty());
}

pub fn assert_address_equiv(a: &Address, b: &Address) {
    assert_eq!(a.name, b.name, "name mismatch");
    assert_eq!(
        a.street_address, b.street_address,
        "street_address mismatch"
    );
    assert_eq!(
        a.address_level2, b.address_level2,
        "address_level2 mismatch"
    );
    assert_eq!(a.postal_code, b.postal_code, "postal_code mismatch");
    assert_eq!(a.country, b.country, "country mismatch");
}

pub fn sync_credit_cards(client: &mut TestClient, local_enc_key: String) -> Result<()> {
    let engine_name = "creditcards";
    let mut local_encryption_keys = HashMap::new();
    local_encryption_keys.insert(engine_name.to_string(), local_enc_key);

    client.sync(&[engine_name.to_string()], local_encryption_keys)?;
    Ok(())
}

pub fn add_credit_card(
    s: &AutofillStore,
    c: UpdatableCreditCardFields,
) -> AutofillResult<CreditCard> {
    let id = s.add_credit_card(c)?.guid;
    Ok(s.get_credit_card(id).expect("Credit card has been added"))
}

pub fn scrub_credit_card(s: Arc<AutofillStore>) -> AutofillResult<()> {
    AutofillStore::scrub_encrypted_data(s).expect("scrub_encrypted_data() to succeed");
    Ok(())
}

pub fn delete_credit_card(s: &AutofillStore, c: CreditCard) -> AutofillResult<()> {
    s.delete_credit_card(c.guid)?;
    Ok(())
}

pub fn verify_credit_card(s: &AutofillStore, c: &CreditCard) {
    let equivalent = s
        .get_credit_card(c.guid.clone())
        .expect("get_credit_card() to succeed");
    assert_credit_cards_equiv(&equivalent, c);
}

pub fn verify_credit_card_removal(s: &AutofillStore) {
    let c = s.get_all_credit_cards().expect("no returned credit cards");
    assert!(c.is_empty());
}

pub fn assert_credit_cards_equiv(a: &CreditCard, b: &CreditCard) {
    assert_eq!(a.cc_name, b.cc_name, "cc_name mismatch");
    assert_eq!(a.cc_number_enc, b.cc_number_enc, "cc_number_enc mismatch");
    assert_eq!(
        a.cc_number_last_4, b.cc_number_last_4,
        "cc_number_last_4 mismatch"
    );
    assert_eq!(a.cc_exp_month, b.cc_exp_month, "cc_exp_month mismatch");
    assert_eq!(a.cc_exp_year, b.cc_exp_year, "cc_exp_year mismatch");
    assert_eq!(a.cc_type, b.cc_type, "cc_type mismatch");
}

// Actual tests
fn test_autofill_credit_cards_general(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some credit cards to client0");

    let key = create_autofill_key().expect("encryption key created");

    let cc1 = add_credit_card(
        &c0.autofill_store,
        UpdatableCreditCardFields {
            cc_name: "jane doe".to_string(),
            cc_number_enc: encrypt_string(key.clone(), "2222222222222222".to_string())
                .expect("encrypted cc number for cc1"),
            cc_number_last_4: "1234".to_string(),
            cc_exp_month: 3,
            cc_exp_year: 2022,
            cc_type: "visa".to_string(),
        },
    )
    .expect("add cc1");

    let cc2 = add_credit_card(
        &c0.autofill_store,
        UpdatableCreditCardFields {
            cc_name: "john deer".to_string(),
            cc_number_enc: encrypt_string(key.clone(), "9999999999999999".to_string())
                .expect("encrypted cc number for cc2"),
            cc_number_last_4: "6543".to_string(),
            cc_exp_month: 10,
            cc_exp_year: 2025,
            cc_type: "mastercard".to_string(),
        },
    )
    .expect("add cc2");

    log::info!("Syncing client0");
    sync_credit_cards(c0, key.clone()).expect("c0 sync to work");

    log::info!("Syncing client1");
    sync_credit_cards(c1, key.clone()).expect("c1 sync to work");

    log::info!("Check state");
    verify_credit_card(&c1.autofill_store, &cc1);
    verify_credit_card(&c1.autofill_store, &cc2);

    // clear records
    delete_credit_card(&c0.autofill_store, cc1).expect("cc1 to be deleted from c0");
    delete_credit_card(&c0.autofill_store, cc2).expect("cc2 to be deleted from c0");
    sync_credit_cards(c0, key.clone()).expect("c0 sync to work");
    sync_credit_cards(c1, key).expect("c1 sync to work");
    verify_credit_card_removal(&c0.autofill_store);
    verify_credit_card_removal(&c1.autofill_store);
}

fn test_autofill_credit_cards_with_scrubbed_cards(c0: &mut TestClient, c1: &mut TestClient) {
    let key = create_autofill_key().expect("encryption key created");

    log::info!("Add a credit card to client0");
    let cc3 = add_credit_card(
        &c0.autofill_store,
        UpdatableCreditCardFields {
            cc_name: "jane deer".to_string(),
            cc_number_enc: encrypt_string(key.clone(), "88888888888888".to_string())
                .expect("encrypted cc number for cc3"),
            cc_number_last_4: "6789".to_string(),
            cc_exp_month: 12,
            cc_exp_year: 2027,
            cc_type: "visa".to_string(),
        },
    )
    .expect("add cc3");

    log::info!("CC3 GUID: {}", cc3.clone().guid);

    log::info!("Scrub the credit cards on client0");
    let _ = scrub_credit_card(c0.autofill_store.clone());

    log::info!("Syncing client0");
    sync_credit_cards(c0, key.clone()).expect("c0 sync to work");

    // clear records
    delete_credit_card(&c0.autofill_store, cc3.clone()).expect("cc3 to be deleted from c0");
    sync_credit_cards(c0, key.clone()).expect("c0 sync to work");
    verify_credit_card_removal(&c0.autofill_store);
    verify_credit_card_removal(&c1.autofill_store);
}

fn test_autofill_addresses_general(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some addresses to client0");

    let a1 = add_address(
        &c0.autofill_store,
        UpdatableAddressFields {
            name: "jane elliott doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            postal_code: "60007".to_string(),
            country: "United States".to_string(),
            ..UpdatableAddressFields::default()
        },
    )
    .expect("add a1");

    let a2 = add_address(
        &c0.autofill_store,
        UpdatableAddressFields {
            name: "john elliott doe".to_string(),
            street_address: "1300 Broadway".to_string(),
            address_level2: "New York, NY".to_string(),
            postal_code: "10001".to_string(),
            country: "United States".to_string(),

            ..UpdatableAddressFields::default()
        },
    )
    .expect("add a2");

    log::info!("Syncing client0");
    sync_addresses(c0).expect("c0 sync to work");

    log::info!("Syncing client1");
    sync_addresses(c1).expect("c1 sync to work");

    log::info!("Check state");
    verify_address(&c1.autofill_store, &a1);
    verify_address(&c1.autofill_store, &a2);

    // clear records
    delete_address(&c0.autofill_store, a1).expect("a1 to be deleted from c0");
    delete_address(&c0.autofill_store, a2).expect("a2 to be deleted from c0");
    sync_addresses(c0).expect("c0 sync to work");
    sync_addresses(c1).expect("c1 sync to work");
    verify_address_removal(&c0.autofill_store);
    verify_address_removal(&c1.autofill_store);
}

pub fn get_test_group() -> TestGroup {
    TestGroup::new(
        "autofill",
        vec![
            (
                "test_autofill_addresses_general",
                test_autofill_addresses_general,
            ),
            (
                "test_autofill_credit_cards_general",
                test_autofill_credit_cards_general,
            ),
            (
                "test_autofill_credit_cards_with_scrubbed_cards",
                test_autofill_credit_cards_with_scrubbed_cards,
            ),
        ],
    )
}
