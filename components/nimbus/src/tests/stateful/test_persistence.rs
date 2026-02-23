/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::fs;

use rkv::StoreOptions;
use serde_json::json;

use crate::Experiment;
use crate::enrollment::ExperimentEnrollment;
use crate::error::{Result, debug};
use crate::stateful::enrollment::{get_experiment_participation, get_rollout_participation};
use crate::stateful::persistence::*;

#[test]
fn test_db_upgrade_no_version() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let rkv = Database::open_rkv(&tmp_dir)?;
    let _meta_store = rkv.open_single("meta", StoreOptions::create())?;
    let experiment_store =
        SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
    let enrollment_store =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer = rkv.write()?;
    enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
    experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
    writer.commit()?;

    let db = Database::new(&tmp_dir)?;
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
    assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
    assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

    Ok(())
}

#[test]
fn test_db_upgrade_unknown_version() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let rkv = Database::open_rkv(&tmp_dir)?;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let experiment_store =
        SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
    let enrollment_store =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer = rkv.write()?;
    meta_store.put(&mut writer, DB_KEY_DB_VERSION, &u16::MAX)?;
    enrollment_store.put(&mut writer, "foo", &"bar".to_owned())?;
    experiment_store.put(&mut writer, "bobo", &"tron".to_owned())?;
    writer.commit()?;
    let db = Database::new(&tmp_dir)?;
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(DB_VERSION));
    assert!(db.collect_all::<String>(StoreId::Enrollments)?.is_empty());
    assert!(db.collect_all::<String>(StoreId::Experiments)?.is_empty());

    Ok(())
}

#[test]
fn test_corrupt_db() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;

    let db_dir = tmp_dir.path().join("db");
    fs::create_dir(db_dir.clone())?;

    // The database filename differs depending on the rkv mode.
    #[cfg(feature = "rkv-safe-mode")]
    let db_file = db_dir.join("data.safe.bin");
    #[cfg(not(feature = "rkv-safe-mode"))]
    let db_file = db_dir.join("data.mdb");

    let garbage = b"Not a database!";
    let garbage_len = garbage.len() as u64;
    fs::write(&db_file, garbage)?;
    assert_eq!(fs::metadata(&db_file)?.len(), garbage_len);
    // Opening the DB should delete the corrupt file and replace it.
    Database::new(&tmp_dir)?;
    // Old contents should be removed and replaced with actual data.
    assert_ne!(fs::metadata(&db_file)?.len(), garbage_len);
    Ok(())
}

// XXX secure-gold has some fields. Ideally, we would also have an
// experiment with all current fields set, and another with almost no
// optional fields set
fn db_v1_experiments_with_non_empty_features() -> Vec<serde_json::Value> {
    vec![
        json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold", // change when copy/pasting to make experiments
            "endDate": null,
            "featureIds": ["abc"], // change when copy/pasting to make experiments
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "abc", // change when copy/pasting to make experiments
                        "enabled": false,
                        "value": {"color": "green"}
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "abc", // change when copy/pasting to make experiments
                        "enabled": true,
                        "value": {}
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedDuration": 21,
            "proposedEnrollment":7,
            "targeting": "true",
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.5.0",
            "slug": "ppop-mobile-test",
            // "arguments": {}, // DEPRECATED
            // "application": "org.mozilla.firefox_beta", // DEPRECATED
            "appName": "fenix",
            "appId": "org.mozilla.firefox_beta",
            "channel": "beta",
            "userFacingName": "[ppop] Mobile test",
            "userFacingDescription": "test",
            "isEnrollmentPaused": false,
            "bucketConfig": {
                "randomizationUnit": "nimbus_id",
                "namespace": "fenix-default-browser-4",
                "start": 0,
                "count": 10000,
                "total": 10000
            },
            "probeSets": [],
            // "outcomes": [], analysis specific, no need to round-trip
            "branches": [
                {
                    "slug": "default_browser_newtab_banner",
                    "ratio": 100,
                    "feature": {
                        "featureId": "fenix-default-browser",
                        "enabled": true,
                        "value": {}
                    }
                },
                {
                    "slug": "default_browser_settings_menu",
                    "ratio": 100,
                    "feature": {
                        "featureId": "fenix-default-browser",
                        "enabled": true,
                        "value": {}
                    }
                },
                {
                    "slug": "default_browser_toolbar_menu",
                    "ratio": 100,
                    "feature": {
                        "featureId": "fenix-default-browser",
                        "enabled": true,
                        "value": {}
                    }
                },
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "fenix-default-browser",
                        "enabled": false,
                        "value": {}
                    }
                }
            ],
            "targeting": "true",
            "startDate": "2021-05-10T12:38:49.699091Z",
            "endDate": null,
            "proposedDuration": 28,
            "proposedEnrollment": 7,
            "referenceBranch": "control",
            "featureIds": [
                "fenix-default-browser"
            ]
        }),
    ]
}
/// Each of this should uniquely reference a single experiment returned
/// from get_db_v1_experiments_with_non_empty_features()
fn get_db_v1_enrollments_with_non_empty_features() -> Vec<serde_json::Value> {
    vec![json!(
        {
            "slug": "secure-gold",
            "status":
                {
                    "Enrolled":
                        {
                            "enrollment_id": "801ee64b-0b1b-44a7-be47-5f1b5c189083", // change when copy/pasting to make new
                            "reason": "Qualified",
                            "branch": "control",
                            "feature_id": "abc" // change on cloning
                        }
                    }
                }
    )]
}

fn get_db_v1_experiments_with_missing_feature_fields() -> Vec<serde_json::Value> {
    vec![
        json!({
            "schemaVersion": "1.0.0",
            "slug": "branch-feature-empty-obj", // change when copy/pasting to make experiments
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {}
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {}
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"branch-feature-empty-obj", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "missing-branch-feature-clause", // change when copy/pasting to make experiments
            "endDate": null,
            "featureIds": ["aaa"], // change when copy/pasting to make experiments
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "aaa", // change when copy/pasting to make experiments
                        "enabled": true,
                        "value": {},
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"empty-branch-feature-clause", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "branch-feature-feature-id-missing", // change when copy/pasting to make experiments
            "endDate": null,
            "featureIds": ["ccc"], // change when copy/pasting to make experiments
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "ccc", // change when copy/pasting to make experiments
                        "enabled": false,
                        "value": {}
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "enabled": true,
                        "value": {}
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"branch-feature-feature-id-missing", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "feature-ids-array-has-empty_string", // change when copy/pasting to make experiments
            "endDate": null,
            "featureIds": [""], // change when copy/pasting to make experiments
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "def", // change when copy/pasting to make experiments
                        "enabled": false,
                        "value": {},
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "def", // change when copy/pasting to make experiments
                        "enabled": true,
                        "value": {}
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"feature-ids-array-has-empty-string", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "missing-feature-ids-in-branch",
            "endDate": null,
            "featureIds": ["abc"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "enabled": true,
                        "value": {}
                    }
                },
                {
                    "slug": "treatment",
                    "ratio": 1,
                    "feature": {
                        "enabled": true,
                        "value": {}
                    }
                }
            ],
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "channel":"nightly",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"no-feature-ids-at-all",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "missing-featureids-array", // change when copy/pasting to make experiments
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome", // change when copy/pasting to make experiments
                        "enabled": false,
                        "value": {}
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "about_welcome", // change when copy/pasting to make experiments
                        "enabled": true,
                        "value": {}
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"valid-feature-experiment", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
        json!({
            "schemaVersion": "1.0.0",
            "slug": "branch-feature-feature-id-empty", // change when copy/pasting to make experiments
            "endDate": null,
            "featureIds": [""], // change when copy/pasting to make experiments
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "", // change when copy/pasting to make experiments
                        "enabled": false,
                        "value": {},
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "", // change when copy/pasting to make experiments
                        "enabled": true,
                        "value": {},
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"branch-feature-feature-id-empty", // change when copy/pasting to make experiments
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        }),
    ]
}

/// Create a database with an old database version number, and
/// populate it with the given experiments and enrollments.
fn create_old_database(
    tmp_dir: &tempfile::TempDir,
    old_version: u16,
    experiments_json: &[serde_json::Value],
    enrollments_json: &[serde_json::Value],
) -> Result<()> {
    error_support::init_for_tests();

    let rkv = Database::open_rkv(tmp_dir)?;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let experiment_store =
        SingleStore::new(rkv.open_single("experiments", StoreOptions::create())?);
    let enrollment_store =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer = rkv.write()?;

    meta_store.put(&mut writer, "db_version", &old_version)?;

    // write out the experiments
    for experiment_json in experiments_json {
        // debug!("experiment_json = {:?}", experiment_json);
        experiment_store.put(
            &mut writer,
            experiment_json["slug"].as_str().unwrap(),
            experiment_json,
        )?;
    }

    // write out the enrollments
    for enrollment_json in enrollments_json {
        // debug!("enrollment_json = {:?}", enrollment_json);
        enrollment_store.put(
            &mut writer,
            enrollment_json["slug"].as_str().unwrap(),
            enrollment_json,
        )?;
    }

    writer.commit()?;
    debug!("create_old_database committed");

    Ok(())
}

/// Migrating v1 to v2 involves finding experiments that
/// don't contain all the feature stuff they should and discarding.
#[test]
fn test_migrate_db_v1_to_db_v2_experiment_discarding() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // write a bunch of invalid experiments
    let db_v1_experiments_with_missing_feature_fields =
        &get_db_v1_experiments_with_missing_feature_fields();

    create_old_database(
        &tmp_dir,
        1,
        db_v1_experiments_with_missing_feature_fields,
        &[],
    )?;

    let db = Database::new(&tmp_dir)?;

    // All of the experiments with invalid FeatureConfig related stuff
    // should have been discarded during migration; leaving us with none.
    let experiments = db.collect_all::<Experiment>(StoreId::Experiments).unwrap();
    debug!("experiments = {:?}", experiments);

    assert_eq!(experiments.len(), 0);

    Ok(())
}

#[test]
fn test_migrate_db_v1_to_db_v2_round_tripping() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // write valid experiments & enrollments
    let db_v1_experiments_with_non_empty_features = &db_v1_experiments_with_non_empty_features();
    // ... and enrollments
    let db_v1_enrollments_with_non_empty_features =
        &get_db_v1_enrollments_with_non_empty_features();

    create_old_database(
        &tmp_dir,
        1,
        db_v1_experiments_with_non_empty_features,
        db_v1_enrollments_with_non_empty_features,
    )?;

    // force an upgrade & read in the upgraded database
    let db = Database::new(&tmp_dir).unwrap();

    // we validate that we can still deserialize the old v1 experiments
    // into the `Experiment` struct
    db.collect_all::<Experiment>(StoreId::Experiments)?;
    // we validate that we can still deserialize the old v1 enrollments
    // into the `ExperimentEnrollment` struct
    db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)?;
    Ok(())
}

/// Migrating db_v1 to db_v2 involves finding enrollments and experiments that
/// don't contain all the feature_id stuff they should and discarding.
#[test]
fn test_migrate_db_v1_with_valid_and_invalid_records_to_db_v2() -> Result<()> {
    let experiment_with_feature = json!({
        "schemaVersion": "1.0.0",
        "slug": "secure-gold",
        "endDate": null,
        "featureIds": ["about_welcome"],
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
                "feature": {
                    "featureId": "about_welcome",
                    "enabled": false
                }
            },
            {
                "slug": "treatment",
                "ratio":1,
                "feature": {
                    "featureId": "about_welcome",
                    "enabled": true
                }
            }
        ],
        "channel": "nightly",
        "probeSets":[],
        "startDate":null,
        "appName": "fenix",
        "appId": "org.mozilla.fenix",
        "bucketConfig":{
            // Setup to enroll everyone by default.
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"secure-gold",
        "last_modified":1_602_197_324_372i64
    });

    let enrollment_with_feature = json!(
        {
            "slug": "secure-gold",
            "status":
                {
                    "Enrolled":
                        {
                            "enrollment_id": "801ee64b-0b1b-44a7-be47-5f1b5c189084",// XXXX should be client id?
                            "reason": "Qualified",
                            "branch": "control",
                            "feature_id": "about_welcome"
                        }
                    }
                }
    );

    let experiment_without_feature = json!(
    {
        "schemaVersion": "1.0.0",
        "slug": "no-features",
        "endDate": null,
        "branches":[
            {
                "slug": "control",
                "ratio": 1,
            },
            {
                "slug": "treatment",
                "ratio": 1,
            }
        ],
        "probeSets":[],
        "startDate":null,
        "appName":"fenix",
        "appId":"org.mozilla.fenix",
        "channel":"nightly",
        "bucketConfig":{
            // Setup to enroll everyone by default.
            "count":10_000,
            "start":0,
            "total":10_000,
            "namespace":"secure-gold",
            "randomizationUnit":"nimbus_id"
        },
        "userFacingName":"Diagnostic test experiment",
        "referenceBranch":"control",
        "isEnrollmentPaused":false,
        "proposedEnrollment":7,
        "userFacingDescription":"This is a test experiment for diagnostic purposes.",
        "id":"no-features",
        "last_modified":1_602_197_324_372i64
    });

    let enrollment_without_feature = json!(
        {
            "slug": "no-features",
            "status":
                {
                    "Enrolled":
                        {
                            "enrollment_id": "801ee64b-0b1b-47a7-be47-5f1b5c189084",
                            "reason": "Qualified",
                            "branch": "control",
                        }
                }
        }
    );

    let tmp_dir = tempfile::tempdir()?;
    error_support::init_for_tests();

    create_old_database(
        &tmp_dir,
        1,
        &[experiment_with_feature, experiment_without_feature],
        &[enrollment_with_feature, enrollment_without_feature],
    )?;

    let db = Database::new(&tmp_dir)?;

    let experiments = db.collect_all::<Experiment>(StoreId::Experiments).unwrap();
    debug!("experiments = {:?}", experiments);

    // The experiment without features should have been discarded, leaving
    // us with only one.
    assert_eq!(experiments.len(), 1);

    let enrollments = db
        .collect_all::<ExperimentEnrollment>(StoreId::Enrollments)
        .unwrap();
    debug!("enrollments = {:?}", enrollments);

    // The enrollment without features should have been discarded, leaving
    // us with only one.
    assert_eq!(enrollments.len(), 1);

    Ok(())
}

#[test]
fn test_migrate_db_v2_to_v3_user_opted_out() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // Create a v2 database where user opted out globally
    create_old_database_v2_with_global_participation(&tmp_dir, false)?;

    // Open with new version - should trigger migration
    let db = Database::new(&tmp_dir)?;

    // Check the database was upgraded to v3
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(3u16));

    // Check that separate flags were set correctly for opted-out user
    let reader = db.read()?;
    assert!(
        !get_experiment_participation(&db, &reader)?, // Should preserve opt-out choice for experiments
    );
    assert!(
        !get_rollout_participation(&db, &reader)?, // Should preserve opt-out choice for rollouts
    );

    // Check old key was removed
    assert_eq!(
        db.get::<bool>(StoreId::Meta, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );

    Ok(())
}

#[test]
fn test_migrate_db_v2_to_v3_user_opted_in() -> Result<()> {
    error_support::init_for_tests();
    let tmp_dir = tempfile::tempdir()?;

    // Create a v2 database where user was opted in globally
    create_old_database_v2_with_global_participation(&tmp_dir, true)?;

    let db = Database::new(&tmp_dir)?;

    // Check the database was upgraded to v3
    assert_eq!(db.get(StoreId::Meta, DB_KEY_DB_VERSION)?, Some(3u16));

    // Check that separate flags were set correctly for opted-in user
    let reader = db.read()?;
    assert!(
        get_experiment_participation(&db, &reader)?, // Should preserve opt-in choice for experiments
    );
    assert!(
        get_rollout_participation(&db, &reader)?, // Should preserve opt-in choice for rollouts
    );

    // Check old key was removed
    assert_eq!(
        db.get::<bool>(StoreId::Meta, DB_KEY_GLOBAL_USER_PARTICIPATION)?,
        None
    );

    Ok(())
}

// Helper function to create a v2 database with global participation flag
fn create_old_database_v2_with_global_participation(
    tmp_dir: &tempfile::TempDir,
    global_participation: bool,
) -> Result<()> {
    let rkv = Database::open_rkv(tmp_dir)?;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let mut writer = rkv.write()?;

    // Set version to 2
    meta_store.put(&mut writer, DB_KEY_DB_VERSION, &2u16)?;

    // Set global participation flag (the old way)
    meta_store.put(
        &mut writer,
        DB_KEY_GLOBAL_USER_PARTICIPATION,
        &global_participation,
    )?;

    writer.commit()?;
    Ok(())
}
