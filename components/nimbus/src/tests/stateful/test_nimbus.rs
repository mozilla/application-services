/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::metrics::EnrollmentStatusExtraDef;
use crate::{
    enrollment::{DisqualifiedReason, EnrolledReason, EnrollmentStatus, ExperimentEnrollment},
    error::Result,
    stateful::{
        behavior::{
            EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter,
            SingleIntervalCounter,
        },
        persistence::{Database, StoreId},
    },
    tests::helpers::{
        get_bucketed_rollout, get_ios_rollout_experiment, get_targeted_experiment,
        to_local_experiments_string, TestMetrics,
    },
    AppContext, AvailableRandomizationUnits, Experiment, NimbusClient, TargetingAttributes,
    DB_KEY_APP_VERSION, DB_KEY_UPDATE_DATE,
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_telemetry_reset() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();
    let mock_exp_slug = "exp-1".to_string();
    let mock_exp_branch = "branch-1".to_string();

    let tmp_dir = tempfile::tempdir()?;
    let client = NimbusClient::new(
        AppContext::default(),
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
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
fn test_installation_date() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();
    let tmp_dir = tempfile::tempdir()?;
    // Step 1: We first test that the SDK will default to using the
    // value in the app context if it exists
    let three_days_ago = Utc::now() - Duration::days(3);
    let time_stamp = three_days_ago.timestamp_millis();
    let mut app_context = AppContext {
        installation_date: Some(time_stamp),
        home_directory: Some(tmp_dir.path().to_str().unwrap().to_string()),
        ..Default::default()
    };
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
    )?;

    client.initialize()?;
    client.apply_pending_experiments()?;

    // We verify that it's three days, ago. Because that's the date
    // passed into the context
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(3)));

    // We now clear the persisted storage
    // to make sure we start from a clear state
    let db = client.db()?;
    let mut writer = db.write()?;
    let store = db.get_store(StoreId::Meta);

    store.clear(&mut writer)?;
    writer.commit()?;

    // Step 2: We test that we will fallback to the
    // filesystem, and if that fails we
    // set Today's date.

    // We recreate our client to make sure
    // we wipe any non-persistent memory
    // this time, with a context that does not
    // include the timestamp
    app_context.installation_date = None;
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
    )?;
    delete_test_creation_date(tmp_dir.path()).ok();
    // When we check the filesystem, we will fail. We haven't `set_test_creation_date`
    // yet.
    client.initialize()?;
    client.apply_pending_experiments()?;
    // We verify that it's today.
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(0)));

    // Step 3: We test that persisted storage takes precedence over
    // checking the filesystem

    // We recreate our client to make sure
    // we wipe any non-persistent memory
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
    )?;
    client.initialize()?;
    // We now store a date for days ago in our file system
    // this shouldn't change the installation date for the nimbus client
    // since client already persisted the date seen earlier.
    let four_days_ago = Utc::now() - Duration::days(4);
    set_test_creation_date(four_days_ago, tmp_dir.path())?;
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    // We will **STILL** get a 0 `days_since_install` since we persisted the value
    // we got on the previous run, therefore we did not check the file system.
    assert!(matches!(targeting_attributes.days_since_install, Some(0)));

    // We now clear the persisted storage
    // to make sure we start from a clear state
    let db = client.db()?;
    let mut writer = db.write()?;
    let store = db.get_store(StoreId::Meta);

    store.clear(&mut writer)?;
    writer.commit()?;

    // Step 4: We test that if the storage is clear, we will fallback to the
    let client = NimbusClient::new(
        app_context,
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    client.initialize()?;
    // now that the store is clear, we will fallback again to the
    // file system, and retrieve the four_days_ago number we stored earlier
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(4)));
    Ok(())
}

#[test]
fn test_days_since_calculation_happens_at_startup() -> Result<()> {
    let metrics = TestMetrics::new();
    // Set up a client with an install date.
    // We'll need two of these, to test the two scenarios.
    let tmp_dir = tempfile::tempdir()?;

    let three_days_ago = Utc::now() - Duration::days(3);
    let time_stamp = three_days_ago.timestamp_millis();
    let app_context = AppContext {
        installation_date: Some(time_stamp),
        home_directory: Some(tmp_dir.path().to_str().unwrap().to_string()),
        ..Default::default()
    };
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        tmp_dir.path(),
        None,
        Default::default(),
        Box::new(metrics.clone()),
    )?;

    // 0. We haven't initialized anything yet, so dates won't be available.
    // In practice this should never happen.
    let targeting_attributes = client.get_targeting_attributes();
    assert!(targeting_attributes.days_since_install.is_none());
    assert!(targeting_attributes.days_since_update.is_none());

    // 1. This is the initialize case, where the app is opened with no Nimbus URL
    // or local experiments. Prior to v94.3, this was the default flow.
    // After v94.3, either initialize() _or_ apply_pending_experiments() could
    // be called.
    client.initialize()?;
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(3)));
    assert!(targeting_attributes.days_since_update.is_some());

    // 2. This is the new case: exactly one of initialize() or apply_pending_experiments()
    // is called during start up.
    // This case ensures that dates are available after apply_pending_experiments().
    let client = NimbusClient::new(
        app_context,
        Default::default(),
        tmp_dir.path(),
        None,
        Default::default(),
        Box::new(metrics),
    )?;
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(3)));
    assert!(targeting_attributes.days_since_update.is_some());

    Ok(())
}

#[test]
fn test_days_since_update_changes_with_context() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();
    let tmp_dir = tempfile::tempdir()?;
    let client = NimbusClient::new(
        AppContext::default(),
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
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
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
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
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id.clone()),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
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
        Default::default(),
        tmp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
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
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: Some(10),
        days_since_update: None,
        is_already_enrolled: false,
        ..Default::default()
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
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: Some(10),
        days_since_update: None,
        is_already_enrolled: false,
        ..Default::default()
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
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: None,
        days_since_update: Some(10),
        is_already_enrolled: false,
        ..Default::default()
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
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        days_since_install: None,
        days_since_update: Some(10),
        is_already_enrolled: false,
        ..Default::default()
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

#[test]
fn event_store_exists_for_apply_pending_experiments() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let db = Database::new(temp_dir.path())?;
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: vec![1, 1, 0].into(),
        },
        config: IntervalConfig::new(3, Interval::Days),
    }]);
    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    event_store.persist_data(&db).ok();

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "test-1",
            "endDate": null,
            "featureIds": ["some-feature-1"],
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
            "targeting": "'app.foregrounded'|eventCountNonZero('Days', 3, 0) > 1",
            "userFacingName": "test experiment",
            "referenceBranch": "control",
            "isEnrollmentPaused": false,
            "proposedEnrollment": 7,
            "userFacingDescription": "This is a test experiment for testing purposes.",
            "id": "secure-copper",
            "last_modified": 1_602_197_324_372i64,
        }, {
            "schemaVersion": "1.0.0",
            "slug": "test-2",
            "endDate": null,
            "featureIds": ["some-feature-2"],
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
            "targeting": "'app.foregrounded'|eventCountNonZero('Days', 3, 0) > 2",
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

    // The number of non-zero days in our event store is 2, so the first experiment
    // should be applied, but the second experiment will not be.
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);
    Ok(())
}

#[test]
fn event_store_on_targeting_attributes_is_updated_after_an_event_is_recorded() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let db = Database::new(temp_dir.path())?;
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 5,
            starting_instant: Utc::now() - Duration::days(1),
            buckets: vec![1, 1, 0, 0, 0].into(),
        },
        config: IntervalConfig::new(5, Interval::Days),
    }]);
    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    event_store.persist_data(&db).ok();

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;
    let experiment_json = serde_json::to_string(&json!({
        "data": [{
            "schemaVersion": "1.0.0",
            "slug": "test-1",
            "endDate": null,
            "featureIds": ["some-feature-1"],
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
            "targeting": "'app.foregrounded'|eventCountNonZero('Days', 5, 0) == 2",
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

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);

    client.record_event("app.foregrounded".to_string(), 1)?;

    client.set_global_user_participation(true)?;

    // The number of non-zero days in our event store is 2, so the first experiment
    // should be applied, but the second experiment will not be.
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

#[test]
fn test_ios_rollout() -> Result<()> {
    let metrics = TestMetrics::new();
    let aru = Default::default();
    let ctx = AppContext {
        app_name: "firefox_ios".to_string(),
        channel: "release".to_string(),
        locale: Some("en-GB".to_string()),
        app_version: Some("114.0".to_string()),
        ..Default::default()
    };
    let tmp_dir = TempDir::new()?;
    let client = NimbusClient::new(
        ctx,
        Default::default(),
        tmp_dir.path(),
        None,
        aru,
        Box::new(metrics),
    )?;

    let exp = get_ios_rollout_experiment();
    let data = json!({
        "data": [
            &exp,
        ]
    });
    client.set_experiments_locally(data.to_string())?;
    client.apply_pending_experiments()?;

    let branch = client.get_experiment_branch(exp.slug)?;
    assert_eq!(branch, Some("control".to_string()));
    client.dump_state_to_log()?;
    Ok(())
}

#[test]
fn test_fetch_enabled() -> Result<()> {
    let metrics = TestMetrics::new();
    let ctx = AppContext {
        app_name: "firefox_ios".to_string(),
        channel: "release".to_string(),
        locale: Some("en-GB".to_string()),
        app_version: Some("114.0".to_string()),
        ..Default::default()
    };
    let tmp_dir = TempDir::new()?;
    let client = NimbusClient::new(
        ctx.clone(),
        Default::default(),
        tmp_dir.path(),
        None,
        Default::default(),
        Box::new(metrics.clone()),
    )?;
    client.set_fetch_enabled(false)?;

    assert!(!client.is_fetch_enabled()?);
    drop(client);

    let client = NimbusClient::new(
        ctx,
        Default::default(),
        tmp_dir.path(),
        None,
        Default::default(),
        Box::new(metrics),
    )?;
    assert!(!client.is_fetch_enabled()?);
    Ok(())
}

#[test]
fn test_active_enrollment_in_targeting() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    // Apply an initial experiment
    let exp = get_targeted_experiment("test-1", "true");
    client.set_experiments_locally(to_local_experiments_string(&[exp])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(targeting_helper.eval_jexl("'test-1' in active_experiments".to_string())?);

    // Apply experiment that targets the above experiment is in enrollments
    let exp = get_targeted_experiment("test-2", "'test-1' in enrollments");
    client.set_experiments_locally(to_local_experiments_string(&[exp])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(!targeting_helper.eval_jexl("'test-1' in active_experiments".to_string())?);
    assert!(targeting_helper.eval_jexl("'test-2' in active_experiments".to_string())?);
    assert!(targeting_helper.eval_jexl("'test-1' in enrollments".to_string())?);
    assert!(targeting_helper.eval_jexl("'test-2' in enrollments".to_string())?);
    assert!(targeting_helper.eval_jexl("enrollments_map['test-1'] == 'treatment'".to_string())?);
    assert!(targeting_helper.eval_jexl("enrollments_map['test-2'] == 'control'".to_string())?);

    Ok(())
}

#[test]
fn test_previous_enrollments_in_targeting() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let slug_1 = "experiment-1-was-enrolled";
    let slug_2 = "experiment-2-dq-not-targeted";
    let slug_3 = "experiment-3-dq-error";
    let slug_4 = "experiment-4-dq-opt-out";
    let slug_5 = "rollout-1-dq-not-selected";

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    // Apply an initial experiment
    let exp_1 = get_targeted_experiment(slug_1, "true");
    let exp_2 = get_targeted_experiment(slug_2, "true");
    let exp_3 = get_targeted_experiment(slug_3, "true");
    let exp_4 = get_targeted_experiment(slug_4, "true");
    let ro_1 = get_bucketed_rollout(slug_5, 10_000);
    client.set_experiments_locally(to_local_experiments_string(&[
        exp_1,
        exp_2,
        exp_3,
        exp_4,
        serde_json::to_value(ro_1)?,
    ])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 5);

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_1))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_2))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_3))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_4))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_5))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_1))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_2))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_3))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_4))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_5))?);

    // Apply empty first experiment, disqualifying second experiment, and decreased bucket rollout
    let exp_2 = get_targeted_experiment(slug_2, "false");
    let exp_3 = get_targeted_experiment(slug_3, "error_out");
    let exp_4 = get_targeted_experiment(slug_4, "true");
    let ro_1 = get_bucketed_rollout(slug_5, 0);
    let experiment_json = serde_json::to_string(
        &json!({"data": [exp_2, exp_3, exp_4, serde_json::to_value(ro_1)?]}),
    )?;
    client.set_experiments_locally(experiment_json)?;
    client.apply_pending_experiments()?;
    client.opt_out(slug_4.to_string())?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);
    let db = client.db()?;
    let enrollments: Vec<ExperimentEnrollment> = db
        .get_store(StoreId::Enrollments)
        .collect_all(&db.write()?)?;
    assert_eq!(enrollments.len(), 5);
    assert!(matches!(
        enrollments.get(0).unwrap(),
        ExperimentEnrollment {
            slug: _slug_1,
            status: EnrollmentStatus::WasEnrolled { .. }
        }
    ));
    assert!(matches!(
        enrollments.get(1).unwrap(),
        ExperimentEnrollment {
            slug: _slug_2,
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::NotTargeted,
                ..
            }
        }
    ));
    assert!(matches!(
        enrollments.get(2).unwrap(),
        ExperimentEnrollment {
            slug: _slug_3,
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::Error,
                ..
            }
        }
    ));
    assert!(matches!(
        enrollments.get(3).unwrap(),
        ExperimentEnrollment {
            slug: _slug_4,
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::OptOut,
                ..
            }
        }
    ));
    assert!(matches!(
        enrollments.get(4).unwrap(),
        ExperimentEnrollment {
            slug: _slug_5,
            status: EnrollmentStatus::Disqualified {
                reason: DisqualifiedReason::NotSelected,
                ..
            }
        }
    ));

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_1))?);
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_2))?);
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_3))?);
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_4))?);
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_5))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_1))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_2))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_3))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_4))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in enrollments", slug_5))?);

    Ok(())
}

#[test]
fn test_opt_out_multiple_experiments_same_feature_does_not_re_enroll() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let slug_1 = "experiment-1";
    let slug_2 = "experiment-2";

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics),
    )?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    let exp_1 = get_targeted_experiment(slug_1, "true");
    let exp_2 = get_targeted_experiment(slug_2, "true");
    client.set_experiments_locally(to_local_experiments_string(&[exp_1, exp_2])?)?;
    client.apply_pending_experiments()?;

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_1))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_2))?);

    client.opt_out(slug_1.into())?;

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_1))?);
    assert!(targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_2))?);

    client.opt_out(slug_2.into())?;

    let targeting_helper = client.create_targeting_helper(None)?;
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_1))?);
    assert!(!targeting_helper.eval_jexl(format!("'{}' in active_experiments", slug_2))?);

    Ok(())
}

#[test]
fn test_enrollment_status_metrics_recorded() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_client_id = "client-1".to_string();

    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        temp_dir.path(),
        None,
        AvailableRandomizationUnits {
            client_id: Some(mock_client_id),
            ..AvailableRandomizationUnits::default()
        },
        Box::new(metrics.clone()),
    )?;
    client.set_nimbus_id(&Uuid::from_str("53baafb3-b800-42ac-878c-c3451e250928")?)?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    let slug_1 = "experiment-1";
    let slug_2 = "experiment-2";
    let slug_3 = "rollout-1";
    let exp_1 = get_targeted_experiment(slug_1, "true");
    let exp_2 = get_targeted_experiment(slug_2, "true");
    let ro_1 = get_bucketed_rollout(slug_3, 10_000);
    client.set_experiments_locally(to_local_experiments_string(&[
        exp_1.clone(),
        exp_2,
        serde_json::to_value(ro_1)?,
    ])?)?;

    client.apply_pending_experiments()?;

    let metric_records: Vec<EnrollmentStatusExtraDef> =
        serde_json::from_value(metrics.assert_get_vec_value("enrollment_status"))?;
    assert_eq!(metric_records.len(), 3);

    assert_eq!(metric_records[0].slug(), slug_1);
    assert_eq!(metric_records[0].status(), "Enrolled");
    assert_eq!(metric_records[0].reason(), "Qualified");
    assert_eq!(metric_records[0].branch(), "treatment");

    assert_eq!(metric_records[1].slug(), slug_2);
    assert_eq!(metric_records[1].status(), "Enrolled");
    assert_eq!(metric_records[1].reason(), "Qualified");
    assert_eq!(metric_records[1].branch(), "control");

    assert_eq!(metric_records[2].slug(), slug_3);
    assert_eq!(metric_records[2].status(), "Enrolled");
    assert_eq!(metric_records[2].reason(), "Qualified");
    assert_eq!(metric_records[2].branch(), "control");

    let slug_4 = "experiment-3";
    let exp_2 = get_targeted_experiment(slug_2, "false");
    let ro_1 = get_bucketed_rollout(slug_3, 0);
    let exp_4 = get_targeted_experiment(slug_4, "blah");
    client.set_experiments_locally(to_local_experiments_string(&[
        exp_2,
        serde_json::to_value(ro_1)?,
        exp_4,
    ])?)?;
    client.apply_pending_experiments()?;

    let metric_records: Vec<EnrollmentStatusExtraDef> =
        serde_json::from_value(metrics.assert_get_vec_value("enrollment_status"))?;
    assert_eq!(metric_records.len(), 6);

    assert_eq!(metric_records[3].slug(), slug_2);
    assert_eq!(metric_records[3].status(), "Disqualified");
    assert_eq!(metric_records[3].reason(), "NotTargeted");
    assert_eq!(metric_records[3].branch(), "control");

    assert_eq!(metric_records[4].slug(), slug_4);
    assert_eq!(metric_records[4].status(), "Error");
    assert_eq!(
        metric_records[4].error_string(),
        "EvaluationError: Identifier 'blah' is undefined"
    );

    assert_eq!(metric_records[5].slug(), slug_3);
    assert_eq!(metric_records[5].status(), "Disqualified");
    assert_eq!(metric_records[5].reason(), "NotSelected");
    assert_eq!(metric_records[5].branch(), "control");

    Ok(())
}
