/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing lib.rs

use chrono::{DateTime, Utc};
use serde_json::json;
use tempdir::TempDir;

use nimbus::{
    enrollment::{EnrolledReason, EnrollmentStatus, ExperimentEnrollment},
    error::Result,
    persistence::StoreId,
    AppContext, AvailableRandomizationUnits, Experiment, NimbusClient, TargetingAttributes,
    DB_KEY_APP_VERSION, DB_KEY_UPDATE_DATE,
};

use std::{io::Write, path::Path};

// #[test]
// fn test_installation_date() -> Result<()> {
//     let mock_client_id = "client-1".to_string();
//     let tmp_dir = TempDir::new("test_installation_date")?;
//     // Step 1: We first test that the SDK will default to using the
//     // value in the app context if it exists
//     let three_days_ago = Utc::now() - Duration::days(3);
//     let time_stamp = three_days_ago.timestamp_millis();
//     let mut app_context = AppContext {
//         installation_date: Some(time_stamp),
//         home_directory: Some(tmp_dir.path().to_str().unwrap().to_string()),
//         ..Default::default()
//     };
//     let client = NimbusClient::new(
//         app_context.clone(),
//         tmp_dir.path(),
//         None,
//         AvailableRandomizationUnits {
//             client_id: Some(mock_client_id.clone()),
//             ..AvailableRandomizationUnits::default()
//         },
//     )?;

//     client.initialize()?;
//     client.apply_pending_experiments()?;

//     // We verify that it's three days, ago. Because that's the date
//     // passed into the context
//     let targeting_attributes = client.get_targeting_attributes();
//     assert!(matches!(targeting_attributes.days_since_install, Some(3)));

//     // We now clear the persisted storage
//     // to make sure we start from a clear state
//     let db = client.db()?;
//     let mut writer = db.write()?;
//     let store = db.get_store(StoreId::Meta);

//     store.clear(&mut writer)?;
//     writer.commit()?;

//     // Step 2: We test that we will fallback to the
//     // filesystem, and if that fails we
//     // set Today's date.

//     // We recreate our client to make sure
//     // we wipe any non-persistent memory
//     // this time, with a context that does not
//     // include the timestamp
//     app_context.installation_date = None;
//     let client = NimbusClient::new(
//         app_context.clone(),
//         tmp_dir.path(),
//         None,
//         AvailableRandomizationUnits {
//             client_id: Some(mock_client_id.clone()),
//             ..AvailableRandomizationUnits::default()
//         },
//     )?;
//     delete_test_creation_date(tmp_dir.path()).ok();
//     // When we check the filesystem, we will fail. We haven't `set_test_creation_date`
//     // yet.
//     client.initialize()?;
//     client.apply_pending_experiments()?;
//     // We verify that it's today.
//     let targeting_attributes = client.get_targeting_attributes();
//     assert!(matches!(targeting_attributes.days_since_install, Some(0)));

//     // Step 3: We test that persisted storage takes precedence over
//     // checking the filesystem

//     // We recreate our client to make sure
//     // we wipe any non-persistent memory
//     let client = NimbusClient::new(
//         app_context.clone(),
//         tmp_dir.path(),
//         None,
//         AvailableRandomizationUnits {
//             client_id: Some(mock_client_id.clone()),
//             ..AvailableRandomizationUnits::default()
//         },
//     )?;
//     client.initialize()?;
//     // We now store a date for days ago in our file system
//     // this shouldn't change the installation date for the nimbus client
//     // since client already persisted the date seen earlier.
//     let four_days_ago = Utc::now() - Duration::days(4);
//     set_test_creation_date(four_days_ago, tmp_dir.path())?;
//     client.apply_pending_experiments()?;
//     let targeting_attributes = client.get_targeting_attributes();
//     // We will **STILL** get a 0 `days_since_install` since we persisted the value
//     // we got on the previous run, therefore we did not check the file system.
//     assert!(matches!(targeting_attributes.days_since_install, Some(0)));

//     // We now clear the persisted storage
//     // to make sure we start from a clear state
//     let db = client.db()?;
//     let mut writer = db.write()?;
//     let store = db.get_store(StoreId::Meta);

//     store.clear(&mut writer)?;
//     writer.commit()?;

//     // Step 4: We test that if the storage is clear, we will fallback to the
//     let client = NimbusClient::new(
//         app_context,
//         tmp_dir.path(),
//         None,
//         AvailableRandomizationUnits {
//             client_id: Some(mock_client_id),
//             ..AvailableRandomizationUnits::default()
//         },
//     )?;
//     client.initialize()?;
//     // now that the store is clear, we will fallback again to the
//     // file system, and retrieve the four_days_ago number we stored earlier
//     client.apply_pending_experiments()?;
//     let targeting_attributes = client.get_targeting_attributes();
//     assert!(matches!(targeting_attributes.days_since_install, Some(4)));
//     Ok(())
// }

#[test]
fn test_telemetry_reset() -> Result<()> {
    let mock_client_id = "client-1".to_string();
    let mock_exp_slug = "exp-1".to_string();
    let mock_exp_branch = "branch-1".to_string();

    let tmp_dir = TempDir::new("test_telemetry_reset")?;
    let client = NimbusClient::new(
        AppContext::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
    )?;

    let get_client_id = || {
        client
            .mutable_state
            .lock()
            .unwrap()
            .available_randomization_units
            .client_id
            .clone()
    };

    // Mock being enrolled in a single experiment.
    let db = client.db()?;
    let mut writer = db.write()?;
    db.get_store(StoreId::Experiments).put(
        &mut writer,
        &mock_exp_slug,
        &Experiment {
            slug: mock_exp_slug.clone(),
            ..Experiment::default()
        },
    )?;
    db.get_store(StoreId::Enrollments).put(
        &mut writer,
        &mock_exp_slug,
        &ExperimentEnrollment {
            slug: mock_exp_slug.clone(),
            status: EnrollmentStatus::new_enrolled(EnrolledReason::Qualified, &mock_exp_branch),
        },
    )?;
    writer.commit()?;

    client.initialize()?;

    // Check expected state before resetting telemetry.
    let orig_nimbus_id = client.nimbus_id()?;
    assert_eq!(get_client_id(), Some(mock_client_id));

    let events = client.reset_telemetry_identifiers(AvailableRandomizationUnits::default())?;

    // We should have reset our nimbus_id.
    assert_ne!(orig_nimbus_id, client.nimbus_id()?);

    // We should have updated the randomization units.
    assert_eq!(get_client_id(), None);

    // We should have been disqualified from the enrolled experiment.
    assert_eq!(client.get_experiment_branch(mock_exp_slug)?, None);

    // We should have returned a single event.
    assert_eq!(events.len(), 1);

    Ok(())
}

#[test]
fn test_days_since_update_changes_with_context() -> Result<()> {
    let mock_client_id = "client-1".to_string();
    let tmp_dir = TempDir::new("test_days_since_update")?;
    let client = NimbusClient::new(
        AppContext::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    client.initialize()?;

    // Step 1: Test what happens if we have no persisted app version,
    // but we got a new version in our app_context.
    // We should set our update date to today.

    // We re-create the client, with an app context that includes
    // a version
    let mut app_context = AppContext {
        app_version: Some("v94.0.0".into()),
        ..Default::default()
    };
    let client = NimbusClient::new(
        app_context.clone(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    client.initialize()?;
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    // The days_since_update should be zero
    assert!(matches!(targeting_attributes.days_since_update, Some(0)));
    let db = client.db()?;
    let reader = db.read()?;
    let store = db.get_store(StoreId::Meta);
    let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
    // we make sure we persisted the version we saw
    assert_eq!(app_version, "v94.0.0");
    let update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
    let diff_with_today = Utc::now() - update_date;
    // we make sure the persisted date, is today
    assert_eq!(diff_with_today.num_days(), 0);

    // Step 2: Test what happens if there is already a persisted date
    // but we get a new one in our context that is the **same**
    // the update_date should not change
    let client = NimbusClient::new(
        app_context.clone(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    client.initialize()?;
    client.apply_pending_experiments()?;

    // We repeat the same tests we did above first
    let targeting_attributes = client.get_targeting_attributes();
    // The days_since_update should still be zero
    assert!(matches!(targeting_attributes.days_since_update, Some(0)));
    let db = client.db()?;
    let reader = db.read()?;
    let store = db.get_store(StoreId::Meta);
    let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
    // we make sure we persisted the version we saw
    assert_eq!(app_version, "v94.0.0");
    let new_update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
    // we make sure the persisted date, is **EXACTLY** the same
    // one we persisted earler, not that the `DateTime` object here
    // includes time to the nanoseconds, so this is a valid way
    // to ensure the objects are the same
    assert_eq!(new_update_date, update_date);

    // Step 3: Test what happens if there is a persisted date,
    // but the app_context includes a newer date, the update_date
    // should be updated

    app_context.app_version = Some("v94.0.1".into()); // A different version
    let client = NimbusClient::new(
        app_context,
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    client.initialize()?;
    client.apply_pending_experiments()?;

    // We repeat some of the same tests we did above first
    let targeting_attributes = client.get_targeting_attributes();
    // The days_since_update should still be zero
    assert!(matches!(targeting_attributes.days_since_update, Some(0)));
    let db = client.db()?;
    let reader = db.read()?;
    let store = db.get_store(StoreId::Meta);
    let app_version: String = store.get(&reader, DB_KEY_APP_VERSION)?.unwrap();
    // we make sure we persisted the **NEW** version we saw
    assert_eq!(app_version, "v94.0.1");
    let new_update_date: DateTime<Utc> = store.get(&reader, DB_KEY_UPDATE_DATE)?.unwrap();
    // we make sure the persisted date is newer and different
    // than the old one. This helps us ensure that there was indeed
    // an update to the date
    assert!(new_update_date > update_date);

    Ok(())
}

#[test]
fn test_days_since_install() -> Result<()> {
    let mock_client_id = "client-1".to_string();

    let temp_dir = TempDir::new("test_days_since_install_failed")?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: Some(10),
        days_since_update: None,
        is_already_enrolled: false,
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some-feature"],
            "branches": [
                {
                "slug": "control",
                "ratio": 1
                },
                {
                "slug": "treatment",
                "ratio": 1
                }
            ],
            "channel": "nightly",
            "probeSets": [],
            "startDate": null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig": {
                "count": 10000,
                "start": 0,
                "total": 10000,
                "namespace": "secure-gold",
                "randomizationUnit": "nimbus_id"
            },
            "targeting": "days_since_install == 10",
            "userFacingName": "test experiment",
            "referenceBranch": "control",
            "isEnrollmentPaused": false,
            "proposedEnrollment": 7,
            "userFacingDescription": "This is a test experiment for testing purposes.",
            "id": "secure-copper",
            "last_modified": 1_602_197_324_372i64,
        }
    ]}))?;
    client.set_experiments_locally(experiment_json)?;
    client.apply_pending_experiments()?;

    // The targeting targeted days_since_install == 10, which is true in the client
    // so we should be enrolled in that experiment
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);
    assert_eq!(active_experiments[0].slug, "secure-gold");
    Ok(())
}

#[test]
fn test_days_since_install_failed_targeting() -> Result<()> {
    let mock_client_id = "client-1".to_string();

    let temp_dir = TempDir::new("test_days_since_install_failed")?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: Some(10),
        days_since_update: None,
        is_already_enrolled: false,
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some-feature"],
            "branches": [
                {
                "slug": "control",
                "ratio": 1
                },
                {
                "slug": "treatment",
                "ratio": 1
                }
            ],
            "channel": "nightly",
            "probeSets": [],
            "startDate": null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig": {
                "count": 10000,
                "start": 0,
                "total": 10000,
                "namespace": "secure-gold",
                "randomizationUnit": "nimbus_id"
            },
            "targeting": "days_since_install < 10",
            "userFacingName": "test experiment",
            "referenceBranch": "control",
            "isEnrollmentPaused": false,
            "proposedEnrollment": 7,
            "userFacingDescription": "This is a test experiment for testing purposes.",
            "id": "secure-copper",
            "last_modified": 1_602_197_324_372i64,
        }
    ]}))?;
    client.set_experiments_locally(experiment_json)?;
    client.apply_pending_experiments()?;

    // The targeting targeted days_since_install < 10, which is false in the client
    // so we should be enrolled in that experiment
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);
    Ok(())
}

#[test]
fn test_days_since_update() -> Result<()> {
    let mock_client_id = "client-1".to_string();

    let temp_dir = TempDir::new("test_days_since_update")?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: None,
        days_since_update: Some(10),
        is_already_enrolled: false,
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some-feature"],
            "branches": [
                {
                "slug": "control",
                "ratio": 1
                },
                {
                "slug": "treatment",
                "ratio": 1
                }
            ],
            "channel": "nightly",
            "probeSets": [],
            "startDate": null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig": {
                "count": 10000,
                "start": 0,
                "total": 10000,
                "namespace": "secure-gold",
                "randomizationUnit": "nimbus_id"
            },
            "targeting": "days_since_update == 10",
            "userFacingName": "test experiment",
            "referenceBranch": "control",
            "isEnrollmentPaused": false,
            "proposedEnrollment": 7,
            "userFacingDescription": "This is a test experiment for testing purposes.",
            "id": "secure-copper",
            "last_modified": 1_602_197_324_372i64,
        }
    ]}))?;
    client.set_experiments_locally(experiment_json)?;
    client.apply_pending_experiments()?;

    // The targeting targeted days_since_update == 10, which is true in the client
    // so we should be enrolled in that experiment
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);
    assert_eq!(active_experiments[0].slug, "secure-gold");
    Ok(())
}

#[test]
fn test_days_since_update_failed_targeting() -> Result<()> {
    let mock_client_id = "client-1".to_string();

    let temp_dir = TempDir::new("test_days_since_update_failed")?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: None,
        days_since_update: Some(10),
        is_already_enrolled: false,
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some-feature"],
            "branches": [
                {
                "slug": "control",
                "ratio": 1
                },
                {
                "slug": "treatment",
                "ratio": 1
                }
            ],
            "channel": "nightly",
            "probeSets": [],
            "startDate": null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig": {
                "count": 10000,
                "start": 0,
                "total": 10000,
                "namespace": "secure-gold",
                "randomizationUnit": "nimbus_id"
            },
            "targeting": "days_since_update < 10",
            "userFacingName": "test experiment",
            "referenceBranch": "control",
            "isEnrollmentPaused": false,
            "proposedEnrollment": 7,
            "userFacingDescription": "This is a test experiment for testing purposes.",
            "id": "secure-copper",
            "last_modified": 1_602_197_324_372i64,
        }
    ]}))?;
    client.set_experiments_locally(experiment_json)?;
    client.apply_pending_experiments()?;

    // The targeting targeted days_since_update < 10, which is false in the client
    // so we should be enrolled in that experiment
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);
    Ok(())
}

fn set_test_creation_date<P: AsRef<Path>>(date: DateTime<Utc>, path: P) -> Result<()> {
    use std::fs::OpenOptions;
    let test_path = path.as_ref().with_file_name("test.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(test_path)
        .unwrap();
    file.write_all(serde_json::to_string(&date).unwrap().as_bytes())?;
    Ok(())
}

fn delete_test_creation_date<P: AsRef<Path>>(path: P) -> Result<()> {
    let test_path = path.as_ref().with_file_name("test.json");
    std::fs::remove_file(test_path)?;
    Ok(())
}
