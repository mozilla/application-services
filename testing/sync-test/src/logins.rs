/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::auth::TestClient;
use crate::testing::TestGroup;
use anyhow::Result;
use logins::encryption::EncryptorDecryptor;
use logins::{
    ApiResult as LoginResult, Login, LoginEntry, LoginFields, LoginStore, SecureLoginFields,
};
use std::collections::HashMap;

// helpers...
// Doesn't check metadata fields
pub fn assert_logins_equiv(a: &Login, b: &Login) {
    assert_eq!(b.guid(), a.guid(), "id mismatch");
    assert_eq!(b.fields, a.fields);
    assert_eq!(b.sec_fields, a.sec_fields);
}

pub fn times_used_for_id(s: &LoginStore, id: &str) -> i64 {
    s.get(id)
        .expect("get() failed")
        .expect("Login doesn't exist")
        .record
        .times_used
}

pub fn add_login(s: &LoginStore, l: LoginEntry, encdec: &EncryptorDecryptor) -> LoginResult<Login> {
    let encrypted = s.add(l, encdec)?;
    let fetched = s
        .get(&encrypted.guid())?
        .expect("Login we just added to exist");
    Ok(fetched.decrypt(encdec).unwrap())
}

pub fn verify_login(s: &LoginStore, l: &Login, encdec: &EncryptorDecryptor) {
    let equivalent = s
        .get(&l.guid())
        .expect("get() to succeed")
        .expect("Expected login to be present")
        .decrypt(encdec)
        .expect("should decrypt");

    assert_logins_equiv(&equivalent, l);
}

pub fn verify_missing_login(s: &LoginStore, id: &str) {
    assert!(
        s.get(id).expect("get() to succeed").is_none(),
        "Login {} should not exist",
        id
    );
}

pub fn update_login<F: FnMut(&mut Login)>(
    s: &LoginStore,
    id: &str,
    encdec: &EncryptorDecryptor,
    mut callback: F,
) -> LoginResult<Login> {
    let encrypted = s.get(id)?.expect("No such login!");
    let mut login = encrypted.decrypt(encdec).unwrap();
    callback(&mut login);
    let to_update = LoginEntry {
        fields: login.fields,
        sec_fields: login.sec_fields,
    };
    s.update(id, to_update, encdec)?;
    Ok(s.get(id)?
        .expect("Just updated this")
        .decrypt(encdec)
        .unwrap())
}

pub fn touch_login(
    s: &LoginStore,
    id: &str,
    times: usize,
    encdec: &EncryptorDecryptor,
) -> LoginResult<Login> {
    for _ in 0..times {
        s.touch(id)?;
    }
    Ok(s.get(id)?.unwrap().decrypt(encdec).unwrap())
}

pub fn sync_logins(client: &mut TestClient, encdec: &EncryptorDecryptor) -> Result<()> {
    let mut local_encryption_keys = HashMap::new();
    local_encryption_keys.insert("passwords".to_string(), encdec.get_key()?);
    client.sync(&["passwords".to_string()], local_encryption_keys)
}

// Actual tests.

fn test_login_general(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some logins to client0");

    let encdec = EncryptorDecryptor::new().unwrap();

    let l0id = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "http://www.example.com".into(),
                form_action_origin: Some("http://login.example.com".into()),
                username_field: "uname".into(),
                password_field: "pword".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username".into(),
                password: "hunter2".into(),
            },
        },
        &encdec,
    )
    .expect("add l0")
    .guid();

    let login0_c0 = touch_login(&c0.logins_store, &l0id, 2, &encdec).expect("touch0 c0");
    assert_eq!(login0_c0.record.times_used, 3);

    let login1_c0 = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "http://www.example.com".into(),
                http_realm: Some("Login".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username".into(),
                password: "sekret".into(),
            },
        },
        &encdec,
    )
    .expect("add l1");
    let l1id = login1_c0.guid();

    log::info!("Syncing client0");
    sync_logins(c0, &encdec).expect("c0 sync to work");

    // Should be the same after syncing.
    verify_login(&c0.logins_store, &login0_c0, &encdec);
    verify_login(&c0.logins_store, &login1_c0, &encdec);

    log::info!("Syncing client1");
    sync_logins(c1, &encdec).expect("c1 sync to work");

    log::info!("Check state");

    verify_login(&c1.logins_store, &login0_c0, &encdec);
    verify_login(&c1.logins_store, &login1_c0, &encdec);

    assert_eq!(
        times_used_for_id(&c1.logins_store, &l0id),
        3,
        "Times used is wrong (first sync)"
    );

    log::info!("Update logins");

    // Change login0 on both
    update_login(&c1.logins_store, &l0id, &encdec, |l| {
        l.sec_fields.password = "testtesttest".into();
    })
    .unwrap();

    let login0_c0 = update_login(&c0.logins_store, &l0id, &encdec, |l| {
        l.fields.username_field = "users_name".into();
    })
    .unwrap();

    // and login1 on remote.
    let login1_c1 = update_login(&c1.logins_store, &l1id, &encdec, |l| {
        l.sec_fields.username = "less_cool_username".into();
    })
    .unwrap();

    log::info!("Sync again");

    sync_logins(c1, &encdec).expect("c1 sync 2");
    sync_logins(c0, &encdec).expect("c0 sync 2");

    log::info!("Check state again");

    // Ensure the remotely changed password change made it through
    verify_login(&c0.logins_store, &login1_c1, &encdec);

    // And that the conflicting one did too.
    verify_login(
        &c0.logins_store,
        &Login {
            fields: LoginFields {
                username_field: "users_name".into(),
                ..login0_c0.fields
            },
            sec_fields: SecureLoginFields {
                username: login0_c0.sec_fields.username,
                password: "testtesttest".into(),
            },
            record: login0_c0.record,
        },
        &encdec,
    );

    assert_eq!(
        c0.logins_store
            .get(&l0id)
            .unwrap()
            .unwrap()
            .record
            .times_used,
        5, // initially 1, touched twice, updated twice (on two accounts!
        // doing this right requires 3WM)
        "Times used is wrong (final)"
    );
}

fn test_login_deletes(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some logins to client0");
    let encdec = EncryptorDecryptor::new().unwrap();

    let login0 = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "http://www.example.com".into(),
                form_action_origin: Some("http://login.example.com".into()),
                username_field: "uname".into(),
                password_field: "pword".into(),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username".into(),
                password: "hunter2".into(),
            },
        },
        &encdec,
    )
    .expect("add l0");
    let l0id = login0.guid();

    let login1 = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "http://www.example.com".into(),
                http_realm: Some("Login".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username".into(),
                password: "sekret".into(),
            },
        },
        &encdec,
    )
    .expect("add l1");
    let l1id = login1.guid();

    let login2 = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "https://www.example.org".into(),
                http_realm: Some("Test".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username100".into(),
                password: "123454321".into(),
            },
        },
        &encdec,
    )
    .expect("add l2");
    let l2id = login2.guid();

    let login3 = add_login(
        &c0.logins_store,
        LoginEntry {
            fields: LoginFields {
                origin: "https://www.example.net".into(),
                http_realm: Some("Http Realm".into()),
                ..Default::default()
            },
            sec_fields: SecureLoginFields {
                username: "cool_username99".into(),
                password: "aaaaa".into(),
            },
        },
        &encdec,
    )
    .expect("add l3");
    let l3id = login3.guid();

    log::info!("Syncing client0");

    sync_logins(c0, &encdec).expect("c0 sync to work");

    // Should be the same after syncing.
    verify_login(&c0.logins_store, &login0, &encdec);
    verify_login(&c0.logins_store, &login1, &encdec);
    verify_login(&c0.logins_store, &login2, &encdec);
    verify_login(&c0.logins_store, &login3, &encdec);

    log::info!("Syncing client1");
    sync_logins(c1, &encdec).expect("c1 sync to work");

    log::info!("Check state");
    verify_login(&c1.logins_store, &login0, &encdec);
    verify_login(&c1.logins_store, &login1, &encdec);
    verify_login(&c1.logins_store, &login2, &encdec);
    verify_login(&c1.logins_store, &login3, &encdec);

    // The 4 logins are for the for possible scenarios. All of them should result in the record
    // being deleted.

    // 1. Client A deletes record, client B has no changes (should delete).
    // 2. Client A deletes record, client B has also deleted record (should delete).
    // 3. Client A deletes record, client B has modified record locally (should delete).
    // 4. Same as #3 but in reverse order.

    // case 1. (c1 deletes record, c0 should have deleted on the other side)
    log::info!("Deleting {} from c1", l0id);
    assert!(c1.logins_store.delete(&l0id).expect("Delete should work"));
    verify_missing_login(&c1.logins_store, &l0id);

    // case 2. Both delete l1 separately
    log::info!("Deleting {} from both", l1id);
    assert!(c0.logins_store.delete(&l1id).expect("Delete should work"));
    assert!(c1.logins_store.delete(&l1id).expect("Delete should work"));

    // case 3a. c0 modifies record (c1 will delete it after c0 syncs so the timestamps line up)
    log::info!("Updating {} on c0", l2id);
    let login2_new = update_login(&c0.logins_store, &l2id, &encdec, |l| {
        l.sec_fields.username = "foobar".into();
    })
    .unwrap();

    // case 4a. c1 deletes record (c0 will modify it after c1 syncs so the timestamps line up)
    assert!(c1.logins_store.delete(&l3id).expect("Delete should work"));

    // Sync c1
    log::info!("Syncing c1");
    sync_logins(c1, &encdec).expect("c1 sync to work");
    log::info!("Checking c1 state after sync");

    verify_missing_login(&c1.logins_store, &l0id);
    verify_missing_login(&c1.logins_store, &l1id);
    verify_login(&c1.logins_store, &login2, &encdec);
    verify_missing_login(&c1.logins_store, &l3id);

    log::info!("Update {} on c0", l3id);
    // 4b
    update_login(&c0.logins_store, &l3id, &encdec, |l| {
        l.sec_fields.password = "quux".into();
    })
    .unwrap();

    // Sync c0
    log::info!("Syncing c0");
    sync_logins(c0, &encdec).expect("c0 sync to work");

    log::info!("Checking c0 state after sync");

    verify_missing_login(&c0.logins_store, &l0id);
    verify_missing_login(&c0.logins_store, &l1id);
    verify_login(&c0.logins_store, &login2_new, &encdec);
    verify_missing_login(&c0.logins_store, &l3id);

    log::info!("Delete {} on c1", l2id);
    // 3b
    assert!(c1.logins_store.delete(&l2id).expect("Delete should work"));

    log::info!("Syncing c1");
    sync_logins(c1, &encdec).expect("c1 sync to work");

    log::info!("{} should stay dead", l2id);
    // Ensure we didn't revive it.
    verify_missing_login(&c1.logins_store, &l2id);

    log::info!("Syncing c0");
    sync_logins(c0, &encdec).expect("c0 sync to work");
    log::info!("Should delete {}", l2id);
    verify_missing_login(&c0.logins_store, &l2id);
}

pub fn get_test_group() -> TestGroup {
    TestGroup::new(
        "logins",
        vec![
            ("test_login_general", test_login_general),
            ("test_login_deletes", test_login_deletes),
        ],
    )
}
