/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

use crate::enrollment::{
    DisqualifiedReason, EnrolledReason, EnrollmentChangeEvent, EnrollmentChangeEventType,
    EnrollmentStatus, ExperimentEnrollment, PreviousGeckoPrefState,
};
use crate::error::{Result, info};
use crate::json::PrefValue;
use crate::metrics::{EnrollmentStatusExtraDef, MalformedFeatureConfigExtraDef};
use crate::schema::{Branch, FeatureConfig};
use crate::stateful::behavior::{
    EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter, SingleIntervalCounter,
};
use crate::stateful::firefox_labs::{
    FirefoxLabsEnrollResult, FirefoxLabsEnrollStatus, FirefoxLabsMetadata,
    FirefoxLabsUnenrollResult, FirefoxLabsUnenrollStatus,
};
use crate::stateful::gecko_prefs::{
    GeckoPrefState, OriginalGeckoPref, PrefBranch, PrefEnrollmentData, PrefUnenrollReason,
    create_feature_prop_pref_map,
};
use crate::stateful::persistence::{Database, StoreId};
use crate::stateful::targeting::RecordedContext;
use crate::tests::helpers::{
    TestGeckoPrefHandler, TestMetrics, TestRecordedContext, get_bucketed_rollout,
    get_bucketed_rollout_with_feature, get_firefox_lab, get_firefox_lab_with_feature,
    get_ios_rollout_experiment, get_multi_feature_experiment, get_single_feature_experiment,
    get_single_feature_rollout, get_targeted_experiment, get_targeted_experiment_with_feature,
    sorted_enrollment_change_events, to_local_experiments_string,
};
use crate::{
    AppContext, DB_KEY_APP_VERSION, DB_KEY_UPDATE_DATE, Experiment, NimbusClient,
    TargetingAttributes,
};

#[test]
fn test_telemetry_reset() -> Result<()> {
    let metrics = TestMetrics::new();
    let mock_exp_slug = "exp-1".to_string();
    let mock_exp_branch = "branch-1".to_string();

    let tmp_dir = tempfile::tempdir()?;
    let client = NimbusClient::new(
        AppContext::default(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;

    let get_targeting_attributes_nimbus_id = || {
        client
            .mutable_state
            .lock()
            .unwrap()
            .targeting_attributes
            .nimbus_id
            .clone()
    };

    let get_aru_nimbus_id = || {
        client
            .mutable_state
            .lock()
            .unwrap()
            .available_randomization_units
            .nimbus_id
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
            branches: vec![Branch {
                slug: "control".into(),
                ratio: 1,
                features: Some(vec![FeatureConfig {
                    feature_id: "foo".into(),
                    value: serde_json::Map::new(),
                }]),
                feature: None,
            }],
            feature_ids: vec!["foo".into()],
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
    assert_eq!(
        get_targeting_attributes_nimbus_id().unwrap(),
        orig_nimbus_id.to_string()
    );
    assert_eq!(get_aru_nimbus_id().unwrap(), orig_nimbus_id.to_string());

    let events = client.reset_telemetry_identifiers()?;
    assert_eq!(
        &events,
        &[EnrollmentChangeEvent {
            experiment_slug: "exp-1".into(),
            branch_slug: "branch-1".into(),
            reason: Some("optout".into()),
            change: EnrollmentChangeEventType::Disqualification,
            feature_ids: vec!["foo".into()],
        },]
    );

    // We should have reset our nimbus_id.
    let new_nimbus_id = client.nimbus_id()?;
    assert_ne!(orig_nimbus_id, new_nimbus_id);
    assert_eq!(
        get_targeting_attributes_nimbus_id().unwrap(),
        new_nimbus_id.to_string()
    );
    assert_eq!(get_aru_nimbus_id().unwrap(), new_nimbus_id.to_string());

    // We should have been disqualified from the enrolled experiment.
    assert_eq!(client.get_experiment_branch(mock_exp_slug)?, None);

    Ok(())
}

#[test]
fn test_installation_date() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    // Step 1: We first test that the SDK will default to using the
    // value in the app context if it exists
    let three_days_ago = Utc::now() - Duration::days(3);
    let time_stamp = three_days_ago.timestamp_millis();
    let mut app_context = AppContext {
        installation_date: Some(time_stamp),
        ..Default::default()
    };
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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

    // Step 2: We test that when no installation_date is provided in context
    // and no persisted date exists, we set Today's date.

    // We recreate our client to make sure
    // we wipe any non-persistent memory
    // this time, with a context that does not
    // include the timestamp
    app_context.installation_date = None;
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    // Since no installation_date is in context and storage is cleared,
    // we will default to today's date
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.initialize()?;
    // Since we already persisted the date from the previous run,
    // we should still get 0 days_since_install
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    // We will **STILL** get a 0 `days_since_install` since we persisted the value
    // we got on the previous run.
    assert!(matches!(targeting_attributes.days_since_install, Some(0)));

    // We now clear the persisted storage
    // to make sure we start from a clear state
    let db = client.db()?;
    let mut writer = db.write()?;
    let store = db.get_store(StoreId::Meta);

    store.clear(&mut writer)?;
    writer.commit()?;

    // Step 4: Test with 4 days since installation
    let four_days_ago = Utc::now() - Duration::days(4);
    app_context.installation_date = Some(four_days_ago.timestamp_millis());
    let client = NimbusClient::new(
        app_context,
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.initialize()?;
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(4)));
    Ok(())
}

#[test]
fn test_days_since_calculation_happens_at_startup() -> Result<()> {
    // Set up a client with an install date.
    // We'll need two of these, to test the two scenarios.
    let tmp_dir = tempfile::tempdir()?;

    let three_days_ago = Utc::now() - Duration::days(3);
    let time_stamp = three_days_ago.timestamp_millis();
    let app_context = AppContext {
        installation_date: Some(time_stamp),
        ..Default::default()
    };
    let client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.apply_pending_experiments()?;
    let targeting_attributes = client.get_targeting_attributes();
    assert!(matches!(targeting_attributes.days_since_install, Some(3)));
    assert!(targeting_attributes.days_since_update.is_some());

    Ok(())
}

#[test]
fn test_days_since_update_changes_with_context() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let client = NimbusClient::new(
        AppContext::default(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
    // one we persisted earlier, not that the `DateTime` object here
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_install_time(Utc::now() - Duration::days(10));
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_install_time(Utc::now() - Duration::days(10));
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_update_time(Utc::now() - Duration::days(10));
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_update_time(Utc::now() - Duration::days(10));
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
    let temp_dir = tempfile::tempdir()?;

    let db = Database::new(temp_dir.path(), TestMetrics::new())?;
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
    let temp_dir = tempfile::tempdir()?;

    let db = Database::new(temp_dir.path(), TestMetrics::new())?;
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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

    client.set_experiment_participation(true)?;

    // The number of non-zero days in our event store is 2, so the first experiment
    // should be applied, but the second experiment will not be.
    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);
    Ok(())
}

#[test]
fn test_ios_rollout() -> Result<()> {
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
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
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_fetch_enabled(false)?;

    assert!(!client.is_fetch_enabled()?);
    drop(client);

    let client = NimbusClient::new(
        ctx,
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    assert!(!client.is_fetch_enabled()?);
    Ok(())
}

#[test]
fn test_active_enrollment_in_targeting() -> Result<()> {
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    // Apply an initial experiment
    let exp_1 = get_targeted_experiment_with_feature(slug_1, "true", "feature-1");
    let exp_2 = get_targeted_experiment_with_feature(slug_2, "true", "feature-2");
    let exp_3 = get_targeted_experiment_with_feature(slug_3, "true", "feature-3");
    let exp_4 = get_targeted_experiment_with_feature(slug_4, "true", "feature-4");
    let ro_1 = get_bucketed_rollout_with_feature(slug_5, 10_000, "feature-5");
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
    let exp_2 = get_targeted_experiment_with_feature(slug_2, "false", "feature-2");
    let exp_3 = get_targeted_experiment_with_feature(slug_3, "error_out", "feature-3");
    let exp_4 = get_targeted_experiment_with_feature(slug_4, "true", "feature-6");
    let ro_1 = get_bucketed_rollout_with_feature(slug_5, 0, "feature-5");
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
    println!("{:#?}", enrollments);
    assert!(matches!(
        enrollments.first().unwrap(),
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    let exp_1 = get_targeted_experiment_with_feature(slug_1, "true", "feature-1");
    let exp_2 = get_targeted_experiment_with_feature(slug_2, "true", "feature-2");
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
    let slug_1 = "experiment-1";
    let slug_2 = "experiment-2";
    let slug_3 = "rollout-1";
    let exp_1 = get_targeted_experiment_with_feature(slug_1, "true", "feature-1");
    let exp_2 = get_targeted_experiment_with_feature(slug_2, "true", "feature-2");
    let ro_1 = get_bucketed_rollout_with_feature(slug_3, 10_000, "feature-1");

    let metrics = TestMetrics::new();
    let client = with_metrics(metrics.clone(), "coenrolling-feature")?;
    // force the nimbus_id to ensure we end up in the right branch.
    client.set_nimbus_id(&Uuid::from_str("53baafb3-b800-42ac-878c-c3451e250928")?)?;
    client.set_experiments_locally(to_local_experiments_string(&[
        exp_1,
        exp_2,
        serde_json::to_value(ro_1)?,
    ])?)?;

    client.apply_pending_experiments()?;

    assert_eq!(metrics.get_submit_targeting_context_calls(), 1u64);

    let metric_records = metrics.get_enrollment_statuses();
    assert_eq!(metric_records.len(), 3);

    assert_eq!(metric_records[0].slug.as_ref().unwrap(), slug_1);
    assert_eq!(metric_records[0].status.as_ref().unwrap(), "Enrolled");
    assert_eq!(metric_records[0].reason.as_ref().unwrap(), "Qualified");
    assert_eq!(metric_records[0].branch.as_ref().unwrap(), "treatment");

    assert_eq!(metric_records[1].slug.as_ref().unwrap(), slug_2);
    assert_eq!(metric_records[1].status.as_ref().unwrap(), "Enrolled");
    assert_eq!(metric_records[1].reason.as_ref().unwrap(), "Qualified");
    assert_eq!(metric_records[1].branch.as_ref().unwrap(), "control");

    assert_eq!(metric_records[2].slug.as_ref().unwrap(), slug_3);
    assert_eq!(metric_records[2].status.as_ref().unwrap(), "Enrolled");
    assert_eq!(metric_records[2].reason.as_ref().unwrap(), "Qualified");
    assert_eq!(metric_records[2].branch.as_ref().unwrap(), "control");

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

    assert_eq!(metrics.get_submit_targeting_context_calls(), 2u64);

    let metric_records = metrics.get_enrollment_statuses();
    assert_eq!(metric_records.len(), 6);

    assert_eq!(metric_records[3].slug.as_ref().unwrap(), slug_2);
    assert_eq!(metric_records[3].status.as_ref().unwrap(), "Disqualified");
    assert_eq!(metric_records[3].reason.as_ref().unwrap(), "NotTargeted");
    assert_eq!(metric_records[3].branch.as_ref().unwrap(), "control");

    assert_eq!(metric_records[4].slug.as_ref().unwrap(), slug_4);
    assert_eq!(metric_records[4].status.as_ref().unwrap(), "Error");
    assert_eq!(
        metric_records[4].error_string.as_ref().unwrap(),
        "EvaluationError: Identifier 'blah' is undefined"
    );

    assert_eq!(metric_records[5].slug.as_ref().unwrap(), slug_3);
    assert_eq!(metric_records[5].status.as_ref().unwrap(), "Disqualified");
    assert_eq!(metric_records[5].reason.as_ref().unwrap(), "NotSelected");
    assert_eq!(metric_records[5].branch.as_ref().unwrap(), "control");

    Ok(())
}

#[test]
fn test_enrollment_status_metrics_not_recorded_app_name_mismatch() -> Result<()> {
    let metrics = TestMetrics::new();

    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "not-fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("53baafb3-b800-42ac-878c-c3451e250928")?)?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    let slug_1 = "experiment-1";
    let exp_1 = get_targeted_experiment(slug_1, "true");
    client.set_experiments_locally(to_local_experiments_string(&[exp_1])?)?;

    client.apply_pending_experiments()?;

    assert_eq!(metrics.get_submit_targeting_context_calls(), 1u64);

    let metric_records = metrics.get_enrollment_statuses();
    assert_eq!(metric_records.len(), 0);

    Ok(())
}

#[test]
fn test_enrollment_status_metrics_not_recorded_channel_mismatch() -> Result<()> {
    let metrics = TestMetrics::new();
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "random-channel".to_string(),
        ..Default::default()
    };

    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("53baafb3-b800-42ac-878c-c3451e250928")?)?;

    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.initialize()?;

    let slug_1 = "experiment-1";
    let exp_1 = get_targeted_experiment(slug_1, "true");
    client.set_experiments_locally(to_local_experiments_string(&[exp_1])?)?;

    client.apply_pending_experiments()?;

    assert_eq!(metrics.get_submit_targeting_context_calls(), 1u64);

    let metric_records = metrics.get_enrollment_statuses();
    assert_eq!(metric_records.len(), 0);
    Ok(())
}

fn with_metrics(metrics: Arc<TestMetrics>, coenrolling_feature: &str) -> Result<NimbusClient> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    NimbusClient::new(
        app_context,
        Default::default(),
        vec![coenrolling_feature.to_string()],
        temp_dir.path(),
        metrics,
        None,
        None,
    )
}

#[test]
fn test_feature_activation_events() -> Result<()> {
    let slug_exp = "my-experiment";
    let feature_exp = "experimental-feature";
    let rec_exp = get_single_feature_experiment(slug_exp, feature_exp, json!({}));

    let slug_ro = "my-rollout";
    let feature_ro = "rollout-feature";
    let rec_ro = get_single_feature_rollout(slug_ro, feature_ro, json!({}));

    let slug_coenr = "my-coenrolling";
    let feature_coenr = "coenrolling-feature";
    let rec_coenr = get_single_feature_experiment(slug_coenr, feature_coenr, json!({}));

    let metrics = TestMetrics::new();
    let client = with_metrics(metrics.clone(), feature_coenr)?;
    client.set_experiments_locally(to_local_experiments_string(&[rec_exp, rec_coenr, rec_ro])?)?;
    client.apply_pending_experiments()?;

    let activations = metrics.get_activations();
    assert!(activations.is_empty());

    // Assert that all the experiments are active.
    assert_eq!(
        Some("control".to_string()),
        client.get_experiment_branch(slug_exp.to_string())?
    );
    assert_eq!(
        Some("control".to_string()),
        client.get_experiment_branch(slug_coenr.to_string())?
    );
    assert_eq!(
        Some("control".to_string()),
        client.get_experiment_branch(slug_ro.to_string())?
    );

    // A feature involved in a rollout doesn't fire activation events.
    let _ = client.get_feature_config_variables(feature_ro.to_string());
    let activations = metrics.get_activations();
    assert!(activations.is_empty());

    // Coenrolled features don't fire activation events.
    let _ = client.get_feature_config_variables(feature_coenr.to_string());
    let activations = metrics.get_activations();
    assert!(activations.is_empty());

    // But features involved in a experiment does!
    let _ = client.get_feature_config_variables(feature_exp.to_string());
    let activations = metrics.get_activations();
    assert!(!activations.is_empty());
    assert_eq!(1, activations.len());
    let ev = &activations[0];
    assert_eq!(Some("control"), ev.branch.as_deref());
    assert_eq!(slug_exp, &ev.slug);
    assert_eq!(feature_exp, &ev.feature_id);

    // Next up, check if a feature involved in both a rollout AND an experiment sends an activation event.
    metrics.clear();
    let slug_exp = "my-experiment-2";
    let feature_exp = "experimental-feature";
    let rec_exp = get_single_feature_experiment(slug_exp, feature_exp, json!({}));

    let slug_ro = "my-rollout-2";
    let rec_ro = get_single_feature_rollout(slug_ro, feature_exp, json!({}));

    client.set_experiments_locally(to_local_experiments_string(&[rec_exp, rec_ro])?)?;

    client.apply_pending_experiments()?;

    // Prove to ourselves that activations haven't been sent until feature_config_variables is
    // called.
    let activations = metrics.get_activations();
    assert!(activations.is_empty());

    // Now ask for this feature. Recall it's used in both an experiment and a rollout.
    let _ = client.get_feature_config_variables(feature_exp.to_string());
    let activations = metrics.get_activations();
    assert!(!activations.is_empty());
    assert_eq!(1, activations.len());
    let ev = &activations[0];
    assert_eq!(Some("control"), ev.branch.as_deref());
    assert_eq!(slug_exp, &ev.slug);
    assert_eq!(feature_exp, &ev.feature_id);

    Ok(())
}

#[test]
fn test_malformed_feature_events() -> Result<()> {
    let slug_exp = "my-experiment";
    let feature_exp = "experimental-feature";
    let rec_exp = get_single_feature_experiment(slug_exp, feature_exp, json!({}));

    let slug_ro = "my-rollout";
    let feature_ro = "rollout-feature";
    let rec_ro = get_single_feature_rollout(slug_ro, feature_ro, json!({}));

    let slug_coenr_1 = "my-coenrolling-1";
    let feature_coenr = "coenrolling-feature";
    let rec_coenr_1 = get_single_feature_experiment(slug_coenr_1, feature_coenr, json!({}));

    let slug_coenr_2 = "my-coenrolling-2";
    let rec_coenr_2 = get_single_feature_experiment(slug_coenr_2, feature_coenr, json!({}));

    let metrics = TestMetrics::new();
    let client = with_metrics(metrics.clone(), feature_coenr)?;
    client.set_experiments_locally(to_local_experiments_string(&[
        rec_exp,
        rec_coenr_1,
        rec_coenr_2,
        rec_ro,
    ])?)?;
    client.apply_pending_experiments()?;

    assert!(metrics.get_malformeds().is_empty());

    let part = "my-part";

    // Experiments!
    client.record_malformed_feature_config(feature_exp.to_string(), part.to_string());

    let events = metrics.get_malformeds();
    assert_eq!(1, events.len());

    assert_eq!(
        MalformedFeatureConfigExtraDef {
            slug: Some(slug_exp.to_string()),
            branch: Some("control".to_string()),
            feature_id: feature_exp.to_string(),
            part: part.to_string()
        },
        events[0]
    );

    metrics.clear();

    // Rollouts!
    client.record_malformed_feature_config(feature_ro.to_string(), part.to_string());
    let events = metrics.get_malformeds();
    assert_eq!(1, events.len());

    assert_eq!(
        MalformedFeatureConfigExtraDef {
            slug: Some(slug_ro.to_string()),
            branch: None,
            feature_id: feature_ro.to_string(),
            part: part.to_string()
        },
        events[0]
    );

    metrics.clear();

    // Coenrolling features!
    client.record_malformed_feature_config(feature_coenr.to_string(), part.to_string());
    let events = metrics.get_malformeds();
    assert_eq!(1, events.len());

    assert_eq!(
        MalformedFeatureConfigExtraDef {
            // For coenrolling features, we don't know which recipe to blame,
            // so we send back all the recipes that are involved.
            slug: Some(format!("{slug_coenr_1}+{slug_coenr_2}")),
            branch: None,
            feature_id: feature_coenr.to_string(),
            part: part.to_string()
        },
        events[0]
    );

    Ok(())
}

#[test]
fn test_new_enrollment_in_targeting_mid_run() -> Result<()> {
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
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    let targeting_attributes = TargetingAttributes {
        app_context,
        ..Default::default()
    };
    client.with_targeting_attributes(targeting_attributes);
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug_1 = "test-1";
    let slug_2 = "test-2";
    let slug_3 = "test-3";
    let slug_4 = "test-4";

    // Apply an initial experiment
    let exp_1 = get_targeted_experiment_with_feature(slug_1, "true", "feature-1");
    let exp_2 = get_targeted_experiment_with_feature(
        slug_2,
        &format!("'{}' in active_experiments", slug_1),
        "feature-2",
    );
    let exp_3 = get_targeted_experiment_with_feature(
        slug_3,
        &format!("'{}' in enrollments", slug_1),
        "feature-3",
    );
    let exp_4 = get_targeted_experiment_with_feature(
        slug_4,
        &format!("enrollments_map['{}'] == 'treatment'", slug_1),
        "feature-4",
    );
    client.set_experiments_locally(to_local_experiments_string(&[exp_1, exp_2, exp_3, exp_4])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 4);

    Ok(())
}

#[test]
fn test_recorded_context_recorded() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };
    let recorded_context = TestRecordedContext::new();
    recorded_context.set_context(json!({
        "app_version": "125.0.0",
        "other": "stuff",
    }));
    let metrics = TestMetrics::new();
    let client = NimbusClient::new(
        app_context.clone(),
        Some(recorded_context.clone()),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug_1 = "test-1";

    // Apply an initial experiment
    let exp_1 = get_targeted_experiment(slug_1, "app_version|versionCompare('125.!') >= 0");
    client.set_experiments_locally(to_local_experiments_string(&[exp_1])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);
    assert_eq!(recorded_context.get_record_calls(), 1u64);
    assert_eq!(metrics.get_submit_targeting_context_calls(), 1u64);

    Ok(())
}

#[test]
fn test_recorded_context_event_queries() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };
    let recorded_context = TestRecordedContext::new();
    recorded_context.set_context(json!({
        "app_version": "125.0.0",
        "other": "stuff",
    }));
    recorded_context.set_event_queries(HashMap::from_iter(vec![(
        "TEST_QUERY".to_string(),
        "'event'|eventSum('Days', 1, 0)".into(),
    )]));
    let client = NimbusClient::new(
        app_context,
        Some(recorded_context.clone()),
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        None,
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug_1 = "test-1";

    // Apply an initial experiment
    let exp_1 = get_targeted_experiment(slug_1, "events.TEST_QUERY == 0.0");
    client.set_experiments_locally(to_local_experiments_string(&[exp_1])?)?;
    client.apply_pending_experiments()?;

    info!(
        "{}",
        serde_json::to_string(&recorded_context.get_event_queries())?
    );

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(recorded_context.get_event_query_values()["TEST_QUERY"], 0.0);
    assert_eq!(active_experiments.len(), 1);
    assert_eq!(recorded_context.get_record_calls(), 1u64);

    Ok(())
}

#[test]
fn test_gecko_pref_enrollment() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };

    let pref_state = GeckoPrefState::new("test.pref", None)
        .with_gecko_value(PrefValue::Null)
        .set_by_user();
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));

    let client = NimbusClient::new(
        app_context,
        Some(TestRecordedContext::new()),
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        Some(handler.clone()),
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug_1 = "slug-1";
    let experiment = get_multi_feature_experiment(
        slug_1,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-value"
            }),
        )],
    )
    .with_targeting("'test.pref'|preferenceIsUserSet");

    client.set_experiments_locally(to_local_experiments_string(&[experiment])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);

    let handler_state = handler.state.lock().expect("Unable to lock handler state");
    let prefs = handler_state.prefs_set.clone().unwrap();

    assert_eq!(1, prefs.len());
    assert_eq!(prefs[0].gecko_pref.pref, pref_state.gecko_pref.pref);
    assert_eq!(prefs[0].gecko_value, Some(PrefValue::Null));
    assert_eq!(
        prefs[0].enrollment_value.clone().unwrap().pref_value,
        PrefValue::String("some-value".to_string())
    );
    assert_eq!(
        prefs[0].enrollment_value.clone().unwrap().feature_id,
        "test_feature".to_string()
    );
    assert_eq!(
        prefs[0].enrollment_value.clone().unwrap().variable,
        "test_prop".to_string()
    );

    Ok(())
}

#[test]
fn test_gecko_pref_unenrollment() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };

    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));

    let client = NimbusClient::new(
        app_context,
        Some(TestRecordedContext::new()),
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        Some(handler.clone()),
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let rollout_slug = "rollout-1";
    let mut rollout = get_multi_feature_experiment(
        rollout_slug,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-rollout-value"
            }),
        )],
    )
    .with_targeting("true");
    rollout.is_rollout = true;

    let experiment_slug = "exp-1";
    let experiment = get_multi_feature_experiment(
        experiment_slug,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-experiment-value"
            }),
        )],
    )
    .with_targeting("true");

    client.set_experiments_locally(to_local_experiments_string(&[rollout, experiment])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 2);

    {
        let handler_state = handler.state.lock().expect("Unable to lock handler state");
        let prefs = handler_state.prefs_set.clone().unwrap();

        assert_eq!(1, prefs.len());
        assert_eq!(
            prefs[0].enrollment_value.clone().unwrap().pref_value,
            PrefValue::String("some-experiment-value".to_string())
        );
        assert_eq!(
            prefs[0].enrollment_value.clone().unwrap().feature_id,
            "test_feature".to_string()
        );
        assert_eq!(
            prefs[0].enrollment_value.clone().unwrap().variable,
            "test_prop".to_string()
        );
    }

    let unenroll_events =
        client.unenroll_for_gecko_pref(pref_state, PrefUnenrollReason::FailedToSet)?;

    assert_eq!(
        &sorted_enrollment_change_events(unenroll_events),
        &[
            EnrollmentChangeEvent {
                experiment_slug: "exp-1".into(),
                branch_slug: "control".into(),
                reason: Some("pref_failed_to_set".into()),
                change: EnrollmentChangeEventType::Disqualification,
                feature_ids: vec!["test_feature".into()],
            },
            EnrollmentChangeEvent {
                experiment_slug: "rollout-1".into(),
                branch_slug: "control".into(),
                reason: Some("pref_failed_to_set".into()),
                change: EnrollmentChangeEventType::Disqualification,
                feature_ids: vec!["test_feature".into()],
            },
        ]
    );

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);

    {
        let handler_state = handler.state.lock().expect("Unable to lock handler state");
        let prefs = handler_state.prefs_set.clone().unwrap();

        assert_eq!(0, prefs.len());

        let store = client.gecko_prefs.unwrap();
        let pref_state = store.get_mutable_pref_state();
        assert!(
            pref_state.gecko_prefs_with_state["test_feature"]["test_prop"]
                .enrollment_value
                .is_none()
        );
    }

    Ok(())
}

#[test]
fn test_gecko_pref_unenrollment_reverts() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };

    let pref_state_1 = GeckoPrefState::new("test.pref.1", None).with_gecko_value(PrefValue::Null);
    let pref_state_2 = GeckoPrefState::new("test.pref.2", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![
        ("test_feature", "test_prop", pref_state_1.clone()),
        ("test_feature_2", "test_prop_2", pref_state_2.clone()),
    ]));

    let client = NimbusClient::new(
        app_context,
        Some(TestRecordedContext::new()),
        Default::default(),
        temp_dir.path(),
        TestMetrics::new(),
        Some(handler.clone()),
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let rollout_slug = "rollout-1";
    let mut rollout = get_multi_feature_experiment(
        rollout_slug,
        vec![
            (
                "test_feature",
                json!({
                    "test_prop": "some-rollout-value"
                }),
            ),
            (
                "test_feature_2",
                json!({
                    "test_prop_2": "some-rollout-value-2"
                }),
            ),
        ],
    )
    .with_targeting("true");
    rollout.is_rollout = true;

    let experiment_slug = "exp-1";
    let experiment = get_multi_feature_experiment(
        experiment_slug,
        vec![
            (
                "test_feature",
                json!({
                    "test_prop": "some-experiment-value"
                }),
            ),
            (
                "test_feature_2",
                json!({
                    "test_prop_2": "some-experiment-value-2"
                }),
            ),
        ],
    )
    .with_targeting("true");

    client.set_experiments_locally(to_local_experiments_string(&[rollout, experiment])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 2);

    {
        let handler_state = handler.state.lock().expect("Unable to lock handler state");
        let prefs = handler_state.prefs_set.clone().unwrap();

        assert_eq!(2, prefs.len());
    }

    // Register post enrolled states for reverting
    let enrolled_pref_1;
    let enrolled_pref_2;
    {
        let store = client.gecko_prefs.as_ref().unwrap();
        let pref_store_state = store.get_mutable_pref_state();
        enrolled_pref_1 =
            pref_store_state.gecko_prefs_with_state["test_feature"]["test_prop"].clone();
        enrolled_pref_2 =
            pref_store_state.gecko_prefs_with_state["test_feature_2"]["test_prop_2"].clone();
    }
    client.register_previous_gecko_pref_states(&[enrolled_pref_1, enrolled_pref_2])?;

    let unenroll_events =
        client.unenroll_for_gecko_pref(pref_state_1, PrefUnenrollReason::Changed)?;

    assert_eq!(
        &sorted_enrollment_change_events(unenroll_events),
        &[
            EnrollmentChangeEvent {
                experiment_slug: "exp-1".into(),
                branch_slug: "control".into(),
                reason: Some("pref_changed".into()),
                change: EnrollmentChangeEventType::Disqualification,
                feature_ids: vec!["test_feature".into(), "test_feature_2".into()],
            },
            EnrollmentChangeEvent {
                experiment_slug: "rollout-1".into(),
                branch_slug: "control".into(),
                reason: Some("pref_changed".into()),
                change: EnrollmentChangeEventType::Disqualification,
                feature_ids: vec!["test_feature".into(), "test_feature_2".into()],
            }
        ]
    );

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 0);

    {
        let handler_state = handler.state.lock().expect("Unable to lock handler state");

        let original_prefs_stored = handler_state.original_prefs_state.clone().unwrap();

        assert_eq!(1, original_prefs_stored.len());
        assert_eq!(
            OriginalGeckoPref::from(&pref_state_2).pref,
            original_prefs_stored[0].pref
        );
    }
    Ok(())
}

#[test]
fn register_previous_gecko_pref_states() -> Result<()> {
    let metrics = TestMetrics::new();
    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };
    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));
    let client = NimbusClient::new(
        app_context.clone(),
        Some(TestRecordedContext::new()),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        Some(handler.clone()),
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let experiment_slug_1 = "exp-1";
    let experiment_1 = get_multi_feature_experiment(
        experiment_slug_1,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-experiment-value"
            }),
        )],
    )
    .with_targeting("true");

    let experiment_slug_2 = "exp-2";
    let experiment_2 = get_multi_feature_experiment(
        experiment_slug_2,
        vec![(
            "test_feature_2",
            json!({
                "test_prop": "some-experiment-value"
            }),
        )],
    )
    .with_targeting("true");

    client.set_experiments_locally(to_local_experiments_string(&[experiment_1, experiment_2])?)?;
    client.apply_pending_experiments()?;

    let mut active_experiments = client.get_active_experiments()?;
    active_experiments.sort_by(|a, b| a.slug.cmp(&b.slug));
    assert_eq!(active_experiments.len(), 2);
    assert_eq!(active_experiments[0].slug, experiment_slug_1);
    assert_eq!(active_experiments[1].slug, experiment_slug_2);

    // Shouldn't have a previous state yet
    {
        let db = client.db()?;
        let reader = db.read()?;

        let enrollments: Vec<ExperimentEnrollment> =
            db.get_store(StoreId::Enrollments).collect_all(&reader)?;

        assert_eq!(enrollments.len(), 2);
        let enrollment_1 = enrollments
            .iter()
            .find(|e| e.slug == experiment_slug_1)
            .expect("Should have an ExperimentEnrollment present.");
        assert!(matches!(
            enrollment_1.status,
            EnrollmentStatus::Enrolled {
                prev_gecko_pref_states: None,
                ..
            }
        ));
    }

    let gecko_pref_state_1 = GeckoPrefState::new("some.pref", Some(PrefBranch::Default))
        .with_gecko_value(json!("some-gecko-value"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: experiment_slug_1.to_string(),
            pref_value: json!("enrollment-pref-value"),
            feature_id: "feature_id".into(),
            variable: "variable".into(),
        });

    let gecko_pref_state_2 = GeckoPrefState::new("some.pref.2", Some(PrefBranch::Default))
        .with_gecko_value(json!("some-gecko-value-2"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: experiment_slug_2.to_string(),
            pref_value: json!("enrollment-pref-value-2"),
            feature_id: "feature_id-2".into(),
            variable: "variable-2".into(),
        });

    let gecko_pref_state_3 = GeckoPrefState::new("some.pref", Some(PrefBranch::Default))
        .with_gecko_value(json!("some-gecko-value-3"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: experiment_slug_2.to_string(),
            pref_value: json!("enrollment-pref-value-3"),
            feature_id: "feature_id-3".into(),
            variable: "variable-3".into(),
        });

    let gecko_pref_states = vec![
        gecko_pref_state_1.clone(),
        gecko_pref_state_2.clone(),
        gecko_pref_state_3.clone(),
    ];

    let call_count_before = handler
        .state
        .lock()
        .unwrap()
        .set_gecko_prefs_state_call_count;

    let registration = client.register_previous_gecko_pref_states(&gecko_pref_states);
    assert!(registration.is_ok());

    // Registration must not send pref values to Gecko.
    assert_eq!(
        call_count_before,
        handler
            .state
            .lock()
            .unwrap()
            .set_gecko_prefs_state_call_count
    );

    let db = client.db()?;
    let reader = db.read()?;
    let mut enrollments: Vec<ExperimentEnrollment> =
        db.get_store(StoreId::Enrollments).collect_all(&reader)?;
    enrollments.sort_by(|a, b| a.slug.cmp(&b.slug));
    assert_eq!(active_experiments.len(), 2);

    let prev_gecko_pref_state_1 = PreviousGeckoPrefState {
        original_value: (&gecko_pref_state_1).into(),
        feature_id: "feature_id".into(),
        variable: "variable".into(),
    };

    assert!(matches!(
        enrollments[0].clone().status,
        EnrollmentStatus::Enrolled { prev_gecko_pref_states : Some(ref states), .. }
            if states[0] == prev_gecko_pref_state_1.clone()
    ));

    let prev_gecko_pref_states_1_using_get = client
        .get_previous_gecko_pref_states(experiment_slug_1.to_string())
        .expect("An error occured")
        .expect("Missing states");

    assert_eq!(
        prev_gecko_pref_state_1,
        prev_gecko_pref_states_1_using_get[0]
    );

    let prev_gecko_pref_state_2 = PreviousGeckoPrefState {
        original_value: (&gecko_pref_state_2).into(),
        feature_id: "feature_id-2".into(),
        variable: "variable-2".into(),
    };

    let prev_gecko_pref_state_3 = PreviousGeckoPrefState {
        original_value: (&gecko_pref_state_3).into(),
        feature_id: "feature_id-3".into(),
        variable: "variable-3".into(),
    };

    assert!(matches!(
        enrollments[1].clone().status,
        EnrollmentStatus::Enrolled { prev_gecko_pref_states : Some(ref states), .. }
            if states[0] == prev_gecko_pref_state_2.clone() && states[1] == prev_gecko_pref_state_3.clone()
    ));

    let prev_gecko_pref_states_2_using_get = client
        .get_previous_gecko_pref_states(experiment_slug_2.to_string())
        .expect("An error occured")
        .expect("Missing states");

    assert_eq!(
        prev_gecko_pref_state_2,
        prev_gecko_pref_states_2_using_get[0]
    );

    Ok(())
}

#[test]
fn test_add_prev_gecko_pref_states_for_experiment() -> Result<()> {
    let metrics = TestMetrics::new();
    let temp_dir = tempfile::tempdir()?;
    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        app_version: Some("124.0.0".to_string()),
        ..Default::default()
    };
    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));
    let client = NimbusClient::new(
        app_context.clone(),
        Some(TestRecordedContext::new()),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        Some(handler),
        None,
    )?;
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let experiment_slug = "exp-1";
    let experiment = get_multi_feature_experiment(
        experiment_slug,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-experiment-value"
            }),
        )],
    )
    .with_targeting("true");

    client.set_experiments_locally(to_local_experiments_string(&[experiment])?)?;
    client.apply_pending_experiments()?;

    let active_experiments = client.get_active_experiments()?;
    assert_eq!(active_experiments.len(), 1);

    let original_prev_gecko_pref_states = vec![PreviousGeckoPrefState {
        original_value: OriginalGeckoPref {
            pref: "some.pref".into(),
            branch: PrefBranch::Default,
            value: Some(serde_json::Value::String(String::from("some-gecko-value"))),
        },
        feature_id: "some_control".into(),
        variable: "test_variable".into(),
    }];

    let db = client.db()?;
    let mut writer = db.write()?;
    NimbusClient::add_prev_gecko_pref_state_for_experiment(
        db,
        &mut writer,
        experiment_slug,
        original_prev_gecko_pref_states.clone(),
    )?;
    let enrollments: Vec<ExperimentEnrollment> =
        db.get_store(StoreId::Enrollments).collect_all(&writer)?;

    let experiment_result = enrollments
        .into_iter()
        .find(|e| e.slug == experiment_slug)
        .expect("Should have an Experiment present.");

    assert!(matches!(
        experiment_result.status,
        EnrollmentStatus::Enrolled { prev_gecko_pref_states, .. }
            if prev_gecko_pref_states == Some(original_prev_gecko_pref_states.clone())
    ));

    let reader_result = db
        .get_store(StoreId::Enrollments)
        .get::<ExperimentEnrollment, _>(&writer, experiment_slug)?
        .and_then(|enrollment| {
            if let EnrollmentStatus::Enrolled {
                prev_gecko_pref_states: prev_gecko_pref_state,
                ..
            } = enrollment.status
            {
                prev_gecko_pref_state
            } else {
                None
            }
        })
        .unwrap();

    assert_eq!(reader_result, original_prev_gecko_pref_states.clone());
    Ok(())
}

#[test]
fn test_opt_out_events() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let metrics = TestMetrics::new();
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.with_targeting_attributes(TargetingAttributes {
        app_context,
        ..Default::default()
    });
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug = "slug";
    let experiment = get_targeted_experiment(slug, "true");
    client.set_experiments_locally(to_local_experiments_string(&[experiment])?)?;

    client.apply_pending_experiments()?;
    assert_eq!(client.get_active_experiments()?.len(), 1);

    let events = client.opt_out(slug.into())?;
    assert_eq!(client.get_active_experiments()?.len(), 0);
    assert_eq!(
        &events,
        &[EnrollmentChangeEvent {
            experiment_slug: slug.into(),
            branch_slug: "control".into(),
            reason: Some("optout".into()),
            change: EnrollmentChangeEventType::Disqualification,
            feature_ids: vec!["some-feature-1".into()],
        },]
    );

    Ok(())
}

#[test]
fn test_opt_in_with_branch_events() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let metrics = TestMetrics::new();
    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.with_targeting_attributes(TargetingAttributes {
        app_context,
        ..Default::default()
    });
    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    let slug = "slug";
    let experiment = get_targeted_experiment(slug, "false");
    client.set_experiments_locally(to_local_experiments_string(&[experiment])?)?;

    client.apply_pending_experiments()?;
    assert_eq!(client.get_active_experiments()?.len(), 0);

    let events = client.opt_in_with_branch(slug.into(), "control".into())?;
    assert_eq!(client.get_active_experiments()?.len(), 1);
    assert_eq!(
        &events,
        &[EnrollmentChangeEvent {
            experiment_slug: slug.into(),
            branch_slug: "control".into(),
            reason: None,
            change: EnrollmentChangeEventType::Enrollment,
            feature_ids: vec!["some-feature-1".into()],
        },]
    );

    Ok(())
}

fn setup_firefox_labs_test(
    recipes: &[Experiment],
) -> Result<(tempfile::TempDir, NimbusClient, Arc<TestMetrics>)> {
    let temp_dir = tempfile::tempdir()?;

    let app_context = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let metrics = TestMetrics::new();

    let mut client = NimbusClient::new(
        app_context.clone(),
        Default::default(),
        Default::default(),
        temp_dir.path(),
        metrics.clone(),
        None,
        None,
    )?;
    client.with_targeting_attributes(TargetingAttributes {
        app_context,
        ..Default::default()
    });

    client.set_nimbus_id(&Uuid::from_str("00000000-0000-0000-0000-000000000004")?)?;
    client.initialize()?;

    client.set_experiments_locally(to_local_experiments_string(recipes)?)?;
    client.apply_pending_experiments()?;

    Ok((temp_dir, client, metrics))
}

fn assert_enrolled_experiment_slugs(client: &NimbusClient, expected_slugs: &[&str]) {
    let mut slugs = client
        .get_active_experiments()
        .unwrap()
        .iter()
        .map(|e| e.slug.clone())
        .collect::<Vec<_>>();

    slugs.sort();

    assert_eq!(&slugs, expected_slugs);
}

fn assert_enrolled_reason(client: &NimbusClient, slug: &str, reason: EnrolledReason) {
    assert!(
        &client
            .get_experiment_enrollment(slug)
            .unwrap()
            .unwrap()
            .status
            .is_enrolled_with_reason(reason),
    );
}

fn assert_disqualified_reason(
    client: &NimbusClient,
    slug: &str,
    expected_reason: DisqualifiedReason,
) {
    let status = client
        .get_experiment_enrollment(slug)
        .unwrap()
        .unwrap()
        .status;

    assert!(matches!(
        &status,
        EnrollmentStatus::Disqualified { reason, .. }
        if *reason == expected_reason
    ));
}

#[test]
fn test_firefox_labs_enroll_unenroll() -> Result<()> {
    // This tests the basic cases for enrollment and unenrollment.
    let (_temp_dir, client, metrics) = setup_firefox_labs_test(&[
        get_single_feature_experiment("experiment", "feature-id", json!({})),
        get_single_feature_experiment("rollout", "feature-id", json!({}))
            .patch(json!({ "isRollout": true })),
        get_firefox_lab_with_feature("lab", "lab-feature-1"),
        get_firefox_lab_with_feature("lab-requires-restart", "lab-feature-2")
            .patch(json!({ "requiresRestart": true })),
        get_firefox_lab_with_feature("lab-links", "lab-feature-3")
            .patch(json!({ "firefoxLabsDescriptionLinks": { "feedback": "https://example.com" } })),
        get_firefox_lab_with_feature("lab-not-rollout", "lab-feature-4")
            .patch(json!({ "isRollout": false })),
        get_firefox_lab_with_feature("lab-no-title", "lab-feature-5")
            .patch(json!({ "firefoxLabsTitle": null })),
        get_firefox_lab_with_feature("lab-no-description", "lab-feature-6")
            .patch(json!({ "firefoxLabsDescription": null })),
        get_firefox_lab_with_feature("lab-different-channel", "lab-feature-7")
            .patch(json!({ "channel": "mystery" })),
    ])?;

    assert_eq!(
        &client.get_available_firefox_labs()?,
        &[
            FirefoxLabsMetadata {
                slug: "lab".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-links".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-requires-restart".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            }
        ]
    );

    assert_eq!(
        &metrics.get_enrollment_statuses(),
        &[
            EnrollmentStatusExtraDef {
                slug: Some("experiment".into()),
                status: Some("Enrolled".into()),
                reason: Some("Qualified".into()),
                branch: Some("control".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab-links".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab-no-description".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab-no-title".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab-not-rollout".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("lab-requires-restart".into()),
                status: Some("NotEnrolled".into()),
                reason: Some("FirefoxLabs".into()),
                ..Default::default()
            },
            EnrollmentStatusExtraDef {
                slug: Some("rollout".into()),
                status: Some("Enrolled".into()),
                reason: Some("Qualified".into()),
                branch: Some("control".into()),
                ..Default::default()
            },
        ],
    );

    metrics.clear();
    assert_enrolled_experiment_slugs(&client, &["experiment", "rollout"]);

    // Enroll in a lab and see the change reported in get_available_firefox_labs()
    assert_eq!(
        client.enroll_in_firefox_lab("lab")?,
        FirefoxLabsEnrollResult {
            status: FirefoxLabsEnrollStatus::Enrolled,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "lab".into(),
                branch_slug: "control".into(),
                reason: None,
                change: EnrollmentChangeEventType::Enrollment,
                feature_ids: vec!["lab-feature-1".into()]
            }],
        }
    );

    assert_enrolled_experiment_slugs(&client, &["experiment", "lab", "rollout"]);
    assert_enrolled_reason(&client, "lab", EnrolledReason::FirefoxLabsOptIn);

    // Opt in does not trigger enrollment status.
    assert_eq!(metrics.get_enrollment_statuses(), &[]);

    assert_eq!(
        &client.get_available_firefox_labs()?,
        &[
            FirefoxLabsMetadata {
                slug: "lab".into(),
                enrolled: true,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-links".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-requires-restart".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            }
        ]
    );

    // Attempting to re-enroll does nothing.
    assert_eq!(
        client.enroll_in_firefox_lab("lab")?,
        FirefoxLabsEnrollResult {
            status: FirefoxLabsEnrollStatus::AlreadyEnrolled,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "lab".into(),
                branch_slug: "N/A".into(),
                reason: Some("already-enrolled".into()),
                change: EnrollmentChangeEventType::EnrollFailed,
                feature_ids: vec![]
            }],
        }
    );

    // Unenrolling also should update get_available_firefox_labs()
    assert_eq!(
        client.unenroll_from_firefox_lab("lab")?,
        FirefoxLabsUnenrollResult {
            status: FirefoxLabsUnenrollStatus::Unenrolled,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "lab".into(),
                branch_slug: "control".into(),
                reason: Some("FirefoxLabsOptOut".into()),
                change: EnrollmentChangeEventType::Disqualification,
                feature_ids: vec!["lab-feature-1".into()]
            }],
        }
    );

    // Opt out does not trigger enrollment status.
    assert_eq!(metrics.get_enrollment_statuses(), &[]);

    assert_enrolled_experiment_slugs(&client, &["experiment", "rollout"]);
    assert_disqualified_reason(&client, "lab", DisqualifiedReason::FirefoxLabsOptOut);

    assert_eq!(
        &client.get_available_firefox_labs()?,
        &[
            FirefoxLabsMetadata {
                slug: "lab".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-links".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com".into())
            },
            FirefoxLabsMetadata {
                slug: "lab-requires-restart".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into())
            }
        ]
    );

    // Attempting to re-unenroll does nothing.
    assert_eq!(
        client.unenroll_from_firefox_lab("lab")?,
        FirefoxLabsUnenrollResult {
            status: FirefoxLabsUnenrollStatus::AlreadyUnenrolled,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "lab".into(),
                branch_slug: "N/A".into(),
                reason: Some("already-unenrolled".into()),
                change: EnrollmentChangeEventType::UnenrollFailed,
                feature_ids: vec![]
            }],
        }
    );

    // Attempting to enroll in a non-existant lab.
    assert_eq!(
        client.enroll_in_firefox_lab("unknown")?,
        FirefoxLabsEnrollResult {
            status: FirefoxLabsEnrollStatus::NoExperiment,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "unknown".into(),
                branch_slug: "N/A".into(),
                reason: Some("lab-does-not-exist".into()),
                change: EnrollmentChangeEventType::EnrollFailed,
                feature_ids: vec![]
            }],
        }
    );

    // Attempting to enroll in a non-lab.
    assert_eq!(
        client.enroll_in_firefox_lab("experiment")?,
        FirefoxLabsEnrollResult {
            status: FirefoxLabsEnrollStatus::NotFirefoxLabsOptIn,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "experiment".into(),
                branch_slug: "N/A".into(),
                reason: Some("not-lab".into()),
                change: EnrollmentChangeEventType::EnrollFailed,
                feature_ids: vec![]
            }],
        }
    );

    // Attempt to unenroll from a non-lab as a lab.
    assert_eq!(
        client.unenroll_from_firefox_lab("experiment")?,
        FirefoxLabsUnenrollResult {
            status: FirefoxLabsUnenrollStatus::NotFirefoxLabsOptIn,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "experiment".into(),
                branch_slug: "N/A".into(),
                reason: Some("not-lab".into()),
                change: EnrollmentChangeEventType::UnenrollFailed,
                feature_ids: vec![],
            }],
        }
    );

    Ok(())
}

#[test]
fn test_firefox_labs_feature_conflict() -> Result<()> {
    // This tests the basic cases for enrollment and unenrollment.
    let (_temp_dir, client, _) = setup_firefox_labs_test(&[
        get_single_feature_experiment("rollout", "labs-feature", json!({}))
            .patch(json!({ "isRollout": true })),
        get_firefox_lab("lab"),
    ])?;

    assert_eq!(&client.get_available_firefox_labs()?, &[]);

    assert_eq!(
        client.enroll_in_firefox_lab("lab")?,
        FirefoxLabsEnrollResult {
            status: FirefoxLabsEnrollStatus::FeatureConflict,
            enrollment_change_events: vec![EnrollmentChangeEvent {
                experiment_slug: "lab".into(),
                branch_slug: "N/A".into(),
                reason: Some("feature-conflict".into()),
                change: EnrollmentChangeEventType::EnrollFailed,
                feature_ids: vec![],
            }],
        }
    );

    Ok(())
}

#[test]
fn test_firefox_labs_rollout_opt_out_does_not_unenroll() -> Result<()> {
    let (_temp_dir, client, _) = setup_firefox_labs_test(&[get_firefox_lab("lab")])?;

    assert_eq!(
        client.enroll_in_firefox_lab("lab")?.status,
        FirefoxLabsEnrollStatus::Enrolled
    );

    let events = client.set_rollout_participation(false)?;
    assert_eq!(&events, &[]);

    assert_enrolled_experiment_slugs(&client, &["lab"]);

    Ok(())
}

#[test]
fn test_firefox_labs_reset_telemetry_does_not_unenroll() -> Result<()> {
    let (_temp_dir, client, _) = setup_firefox_labs_test(&[get_firefox_lab("lab")])?;

    assert_eq!(
        client.enroll_in_firefox_lab("lab")?.status,
        FirefoxLabsEnrollStatus::Enrolled
    );

    let events = client.reset_telemetry_identifiers()?;
    assert_eq!(&events, &[]);

    assert_enrolled_experiment_slugs(&client, &["lab"]);

    Ok(())
}

#[test]
fn test_unenroll_from_all_firefox_labs() -> Result<()> {
    let (_temp_dir, client, _) = setup_firefox_labs_test(&[
        get_single_feature_experiment("experiment", "feature-id", json!({})),
        get_single_feature_experiment("rollout", "feature-id", json!({}))
            .patch(json!({ "isRollout": true })),
        get_firefox_lab_with_feature("lab-1", "feature-1"),
        get_firefox_lab_with_feature("lab-2", "feature-2"),
        get_firefox_lab_with_feature("lab-3", "feature-3"),
    ])?;

    assert_enrolled_experiment_slugs(&client, &["experiment", "rollout"]);

    client.enroll_in_firefox_lab("lab-1")?;
    client.enroll_in_firefox_lab("lab-2")?;
    client.enroll_in_firefox_lab("lab-3")?;

    assert_enrolled_experiment_slugs(
        &client,
        &["experiment", "lab-1", "lab-2", "lab-3", "rollout"],
    );

    client.unenroll_from_all_firefox_labs()?;

    assert_enrolled_experiment_slugs(&client, &["experiment", "rollout"]);
    assert_disqualified_reason(&client, "lab-1", DisqualifiedReason::FirefoxLabsOptOut);
    assert_disqualified_reason(&client, "lab-2", DisqualifiedReason::FirefoxLabsOptOut);
    assert_disqualified_reason(&client, "lab-3", DisqualifiedReason::FirefoxLabsOptOut);

    Ok(())
}
