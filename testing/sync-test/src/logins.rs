/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::auth::TestClient;
use crate::testing::TestGroup;
use anyhow::Result;
use logins::{
    encryption::{create_key, ManagedEncryptorDecryptor, StaticKeyManager},
    ApiResult as LoginResult, Login, LoginEntry, LoginStore,
};
use std::sync::Arc;
use std::{collections::hash_map::RandomState, collections::HashMap};
use sync15::ServerTimestamp;

// helpers...
// Doesn't check metadata fields
pub fn assert_logins_equiv(a: &Login, b: &Login) {
    assert_eq!(b.guid(), a.guid(), "id mismatch");
    assert_eq!(b.origin, a.origin);
    assert_eq!(b.form_action_origin, a.form_action_origin);
    assert_eq!(b.http_realm, a.http_realm);
    assert_eq!(b.username_field, a.username_field);
    assert_eq!(b.password_field, a.password_field);
    assert_eq!(b.username, a.username);
    assert_eq!(b.password, a.password);
}

pub fn times_used_for_id(s: &LoginStore, id: &str) -> i64 {
    s.get(id)
        .expect("get() failed")
        .expect("Login doesn't exist")
        .times_used
}

pub fn add_login(s: &LoginStore, l: LoginEntry) -> LoginResult<Login> {
    let login = s.add(l)?;
    let fetched = s.get(&login.guid())?.expect("Login we just added to exist");
    Ok(fetched)
}

pub fn verify_login(s: &LoginStore, l: &Login) {
    let equivalent = s
        .get(&l.guid())
        .expect("get() to succeed")
        .expect("Expected login to be present");

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
    mut callback: F,
) -> LoginResult<Login> {
    let mut login = s.get(id)?.expect("No such login!");
    callback(&mut login);
    let to_update = login.entry();
    s.update(id, to_update)?;
    Ok(s.get(id)?.expect("Just updated this"))
}

pub fn touch_login(s: &LoginStore, id: &str, times: usize) -> LoginResult<Login> {
    for _ in 0..times {
        s.touch(id)?;
    }
    Ok(s.get(id)?.unwrap())
}

pub fn sync_logins(client: &mut TestClient) -> Result<()> {
    let local_encryption_keys = HashMap::new();
    client.sync(&["passwords".to_string()], local_encryption_keys)
}

pub fn sync_logins_with_failure(
    client: &mut TestClient,
) -> Result<HashMap<String, String, RandomState>> {
    let local_encryption_keys = HashMap::new();
    client.sync_with_failure(&["passwords".to_string()], local_encryption_keys)
}

// Actual tests.

fn test_login_general(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some logins to client0");

    let l0id = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example.com".into(),
            form_action_origin: Some("http://login.example.com".into()),
            username_field: "uname".into(),
            password_field: "pword".into(),
            username: "cool_username".into(),
            password: "hunter2".into(),
            ..Default::default()
        },
    )
    .expect("add l0")
    .guid();

    let login0_c0 = touch_login(&c0.logins_store, &l0id, 2).expect("touch0 c0");
    assert_eq!(login0_c0.times_used, 3);

    let login1_c0 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example.com".into(),
            http_realm: Some("Login".into()),
            username: "cool_username".into(),
            password: "sekret".into(),
            ..Default::default()
        },
    )
    .expect("add l1");
    let l1id = login1_c0.guid();

    log::info!("Syncing client0");
    sync_logins(c0).expect("c0 sync to work");

    // Should be the same after syncing.
    verify_login(&c0.logins_store, &login0_c0);
    verify_login(&c0.logins_store, &login1_c0);

    log::info!("Syncing client1");
    sync_logins(c1).expect("c1 sync to work");

    log::info!("Check state");

    verify_login(&c1.logins_store, &login0_c0);
    verify_login(&c1.logins_store, &login1_c0);

    assert_eq!(
        times_used_for_id(&c1.logins_store, &l0id),
        3,
        "Times used is wrong (first sync)"
    );

    log::info!("Update logins");

    // Change login0 on both
    update_login(&c1.logins_store, &l0id, |l| {
        l.password = "testtesttest".into();
    })
    .unwrap();

    let login0_c0 = update_login(&c0.logins_store, &l0id, |l| {
        l.username_field = "users_name".into();
    })
    .unwrap();

    // and login1 on remote.
    let login1_c1 = update_login(&c1.logins_store, &l1id, |l| {
        l.username = "less_cool_username".into();
    })
    .unwrap();

    log::info!("Sync again");

    sync_logins(c1).expect("c1 sync 2");
    sync_logins(c0).expect("c0 sync 2");

    log::info!("Check state again");

    // Ensure the remotely changed password change made it through
    verify_login(&c0.logins_store, &login1_c1);

    // And that the conflicting one did too.
    verify_login(
        &c0.logins_store,
        &Login {
            username_field: "users_name".into(),
            password: "testtesttest".into(),
            ..login0_c0
        },
    );

    assert_eq!(
        c0.logins_store.get(&l0id).unwrap().unwrap().times_used,
        5, // initially 1, touched twice, updated twice (on two accounts!
        // doing this right requires 3WM)
        "Times used is wrong (final)"
    );
}

fn test_login_deletes(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add some logins to client0");

    let login0 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example.com".into(),
            form_action_origin: Some("http://login.example.com".into()),
            username_field: "uname".into(),
            password_field: "pword".into(),
            username: "cool_username".into(),
            password: "hunter2".into(),
            ..Default::default()
        },
    )
    .expect("add l0");
    let l0id = login0.guid();

    let login1 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example.com".into(),
            http_realm: Some("Login".into()),
            username: "cool_username".into(),
            password: "sekret".into(),
            ..Default::default()
        },
    )
    .expect("add l1");
    let l1id = login1.guid();

    let login2 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "https://www.example.org".into(),
            http_realm: Some("Test".into()),
            username: "cool_username100".into(),
            password: "123454321".into(),
            ..Default::default()
        },
    )
    .expect("add l2");
    let l2id = login2.guid();

    let login3 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "https://www.example.net".into(),
            http_realm: Some("Http Realm".into()),
            username: "cool_username99".into(),
            password: "aaaaa".into(),
            ..Default::default()
        },
    )
    .expect("add l3");
    let l3id = login3.guid();

    log::info!("Syncing client0");

    sync_logins(c0).expect("c0 sync to work");

    // Should be the same after syncing.
    verify_login(&c0.logins_store, &login0);
    verify_login(&c0.logins_store, &login1);
    verify_login(&c0.logins_store, &login2);
    verify_login(&c0.logins_store, &login3);

    log::info!("Syncing client1");
    sync_logins(c1).expect("c1 sync to work");

    log::info!("Check state");
    verify_login(&c1.logins_store, &login0);
    verify_login(&c1.logins_store, &login1);
    verify_login(&c1.logins_store, &login2);
    verify_login(&c1.logins_store, &login3);

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
    let login2_new = update_login(&c0.logins_store, &l2id, |l| {
        l.username = "foobar".into();
    })
    .unwrap();

    // case 4a. c1 deletes record (c0 will modify it after c1 syncs so the timestamps line up)
    assert!(c1.logins_store.delete(&l3id).expect("Delete should work"));

    // Sync c1
    log::info!("Syncing c1");
    sync_logins(c1).expect("c1 sync to work");
    log::info!("Checking c1 state after sync");

    verify_missing_login(&c1.logins_store, &l0id);
    verify_missing_login(&c1.logins_store, &l1id);
    verify_login(&c1.logins_store, &login2);
    verify_missing_login(&c1.logins_store, &l3id);

    log::info!("Update {} on c0", l3id);
    // 4b
    update_login(&c0.logins_store, &l3id, |l| {
        l.password = "quux".into();
    })
    .unwrap();

    // Sync c0
    log::info!("Syncing c0");
    sync_logins(c0).expect("c0 sync to work");

    log::info!("Checking c0 state after sync");

    verify_missing_login(&c0.logins_store, &l0id);
    verify_missing_login(&c0.logins_store, &l1id);
    verify_login(&c0.logins_store, &login2_new);
    verify_missing_login(&c0.logins_store, &l3id);

    log::info!("Delete {} on c1", l2id);
    // 3b
    assert!(c1.logins_store.delete(&l2id).expect("Delete should work"));

    log::info!("Syncing c1");
    sync_logins(c1).expect("c1 sync to work");

    log::info!("{} should stay dead", l2id);
    // Ensure we didn't revive it.
    verify_missing_login(&c1.logins_store, &l2id);

    log::info!("Syncing c0");
    sync_logins(c0).expect("c0 sync to work");
    log::info!("Should delete {}", l2id);
    verify_missing_login(&c0.logins_store, &l2id);
}

// This test verifies that the delete_local function deletes a login without propogating the
// deletion to the server
fn test_login_local_delete(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add a login to client0");

    // Add a login
    let login0 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example2.com".into(),
            form_action_origin: Some("http://login.example2.com".into()),
            username_field: "uname".into(),
            password_field: "pword".into(),
            username: "cool_username".into(),
            password: "hunter2".into(),
            ..Default::default()
        },
    )
    .expect("add l0");

    let l0id = login0.guid();

    // Sync the first device where the login was added
    log::info!("Syncing client0 -- inital sync");
    sync_logins(c0).expect("c0 sync to work");

    // Sync the second device
    log::info!("Syncing client1 -- inital sync");
    sync_logins(c1).expect("c0 sync to work");

    // Verify that the login exists on both devices
    verify_login(&c0.logins_store, &login0);
    verify_login(&c1.logins_store, &login0);

    // Delete the login on the first device
    log::info!("Performing local delete {} from c0", l0id);
    c0.logins_store
        .db
        .lock()
        .delete_local_records_for_remote_replacement(vec![&l0id])
        .expect("Local delete should work");

    // Verify that the login no longer exists on the first device
    verify_missing_login(&c0.logins_store, &l0id);

    // Sync the first device
    log::info!("Syncing client0 -- post local deletion");
    sync_logins(c0).expect("c0 sync to work");

    // Sync the second device
    log::info!("Syncing client1 -- post local deletion");
    sync_logins(c1).expect("c1 sync to work");

    // The following verification checks ensure that the delete_local_for_remote_replacement
    // call removed the login from the first device without propogating that change to the
    // sync server.

    // Verify that the login is missing from the first device.
    verify_missing_login(&c0.logins_store, &l0id);

    // Verify that the login still exists on the second device
    verify_login(&c1.logins_store, &login0);

    // Reset the last sync date for the logins sync engine. This will allow the previously synced
    // and then locally deleted record to be downloaded to the first client.
    c0.logins_store
        .clone()
        .set_last_sync(ServerTimestamp(0))
        .expect("last sync to be set");

    // Sync the first device
    log::info!("Syncing client0 -- post c0 reset");
    sync_logins(c0).expect("c0 sync to work");

    // Sync the second device
    log::info!("Syncing client1 -- post c0 reset");
    sync_logins(c1).expect("c1 sync to work");

    // Verify that the login exists on both devices
    verify_login(&c0.logins_store, &login0);
    verify_login(&c1.logins_store, &login0);

    // Clear the stores
    _ = c0.logins_store.wipe_local();
    _ = c1.logins_store.wipe_local();
}

fn test_verify_logins_unblocks_syncing(c0: &mut TestClient, c1: &mut TestClient) {
    log::info!("Add a login to client0");

    // Add a login
    let login0 = add_login(
        &c0.logins_store,
        LoginEntry {
            origin: "http://www.example2.com".into(),
            form_action_origin: Some("http://login.example2.com".into()),
            username_field: "uname".into(),
            password_field: "pword".into(),
            username: "cool_username".into(),
            password: "hunter2".into(),
            ..Default::default()
        },
    )
    .expect("add login0");

    // Sync the first device where the login was added
    log::info!("Syncing client0 -- inital sync");
    sync_logins(c0).expect("c0 sync to work");

    // Sync the second device
    log::info!("Syncing client1 -- inital sync");
    sync_logins(c1).expect("c0 sync to work");

    // Verify that the login exists on both devices
    verify_login(&c0.logins_store, &login0);
    verify_login(&c1.logins_store, &login0);

    // Add a login with a different EncryptorDecryptor to replicate having a stored login that cannot be decrypted
    // with the EncryptorDecryptor property of the store
    let key = create_key().unwrap();
    let new_encdec = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(
        StaticKeyManager::new(key.clone()),
    )));

    log::info!("Add another login to client0");

    let login1 = c0
        .logins_store
        .db
        .lock()
        .add(
            LoginEntry {
                origin: "http://www.example3.com".into(),
                form_action_origin: Some("http://login.example3.com".into()),
                username_field: "uname".into(),
                password_field: "pword".into(),
                username: "cool_username".into(),
                password: "hunter2".into(),
                ..Default::default()
            },
            &*new_encdec,
        )
        .expect("add login1");
    let l1id = login1.guid();

    // Check that the corrupted login exists on first device
    // The db retrieval function is being used instead of the store function so that we
    // can provided our own EncryptorDecryptor.
    let retrieved_login = c0
        .logins_store
        .db
        .lock()
        .get_by_id(&l1id)
        .expect("get_by_id returns successfully")
        .expect("login to be retrieved")
        .decrypt(&*new_encdec)
        .expect("decryption to succeed");
    assert_eq!(retrieved_login.guid(), l1id);

    // Check that syncing after adding a corrupted login fails with a decryption error
    let failures = sync_logins_with_failure(c0).expect("sync to complete with failures");
    let login_failures = failures.get("passwords");
    assert!(login_failures.is_some());
    assert!(login_failures.unwrap().contains("decryption failed"));

    // Execute the verification logic to remove the corrupted login
    log::info!("Verify logins");
    assert!(c0
        .logins_store
        .clone()
        .verify_logins()
        .expect("stored logins to be verified"));

    // Verify that the corrupted login has been removed
    verify_missing_login(&c0.logins_store, &l1id);

    // Sync the first device after verification
    log::info!("Syncing client0 -- after verification");
    sync_logins(c0).expect("c0 sync to work");

    // Sync the second device after verification
    log::info!("Syncing client1 -- after verification");
    sync_logins(c1).expect("c0 sync to work");

    // Verify that the first login record still exists on both devices
    verify_login(&c0.logins_store, &login0);
    verify_login(&c1.logins_store, &login0);

    // Clear the stores
    _ = c0.logins_store.wipe_local();
    _ = c1.logins_store.wipe_local();
}

pub fn get_test_group() -> TestGroup {
    TestGroup::new(
        "logins",
        vec![
            ("test_login_general", test_login_general),
            ("test_login_deletes", test_login_deletes),
            ("test_login_local_delete", test_login_local_delete),
            (
                "test_verify_logins_unblocks_syncing",
                test_verify_logins_unblocks_syncing,
            ),
        ],
    )
}
