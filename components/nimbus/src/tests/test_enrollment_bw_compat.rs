// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(feature = "stateful")]
use rkv::StoreOptions;
use serde_json::json;

use crate::enrollment::*;

cfg_if::cfg_if! {
  if #[cfg(feature = "stateful")] {
      use crate::error::Result;
      use crate::metrics::DatabaseMigrationExtraDef;
      use crate::tests::helpers::TestMetrics;
      use crate::stateful::enrollment::v3;
      use crate::stateful::persistence::{Database, SingleStore, StoreId, DB_KEY_DB_VERSION, DatabaseMigrationReason};
  }
}

/// A suite of tests for b/w compat of data storage schema.
///
/// We use the `Serialize/`Deserialize` impls on various structs in order to persist them
/// into rkv, and it's important that we be able to read previously-persisted data even
/// if the struct definitions change over time.
///
/// This is a suite of tests specifically to check for backward compatibility with data
/// that may have been written to disk by previous versions of the library.
///
/// ⚠️ Warning : Do not change the JSON data used by these tests. ⚠️
/// ⚠️ The whole point of the tests is to check things work with that data. ⚠️
///

#[test]
// This was the `ExperimentEnrollment` object schema as it initially shipped to Fenix Nightly.
// It was missing some fields that have since been added.
fn test_experiment_enrollment_schema_initial_release() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let enroll: ExperimentEnrollment = serde_json::from_value(json!({
        "slug": "test",
        "status": {"Enrolled": {
            "enrollment_id": "b6d6f532-e219-4b5a-8ddf-66700dd47d68",
            "reason": "Qualified",
            "branch": "hello",
        }}
    }))
    .unwrap();
    assert!(matches!(enroll.status, EnrollmentStatus::Enrolled { .. }));
}

// In #96 we added a `feature_id` field to the ExperimentEnrollment schema.
// This tests the data as it was after that change.
#[test]
fn test_experiment_schema_with_feature_ids() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let enroll: ExperimentEnrollment = serde_json::from_value(json!({
        "slug": "secure-gold",
        "status": {"Enrolled": {
            "enrollment_id": "b6d6f532-e219-4b5a-8ddf-66700dd47d68",
            "reason": "Qualified",
            "branch": "hello",
            "feature_id": "some_control"
        }}
    }))
    .unwrap();
    assert!(matches!(enroll.status, EnrollmentStatus::Enrolled { .. }));
}

// In SDK-260 we added a FeatureConflict variant to the NotEnrolledReason
// schema.
#[test]
#[cfg(feature = "stateful")]
fn test_not_enrolled_reason_schema_with_feature_conflict() -> Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let rkv = Database::open_rkv(&tmp_dir)?.0;
    let meta_store = SingleStore::new(rkv.open_single("meta", StoreOptions::create())?);
    let enrollment_store: SingleStore =
        SingleStore::new(rkv.open_single("enrollments", StoreOptions::create())?);
    let mut writer: rkv::Writer<rkv::backend::SafeModeRwTransaction<'_>> = rkv.write()?;

    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let non_enrollment: v3::LegacyExperimentEnrollment = serde_json::from_value(json!({
        "slug": "secure-gold",
        "status": {"NotEnrolled": {
            "reason": "FeatureConflict",
        }}
    }))
    .unwrap();

    meta_store.put(&mut writer, DB_KEY_DB_VERSION, &3)?;
    enrollment_store.put(&mut writer, &non_enrollment.slug, &non_enrollment)?;
    writer.commit()?;

    let metrics = TestMetrics::new();
    let db = Database::new(&tmp_dir, metrics.clone())?;

    assert_eq!(
        metrics.get_database_migration_events(),
        [DatabaseMigrationExtraDef {
            reason: DatabaseMigrationReason::Upgrade.to_string(),
            from_version: 3,
            to_version: 4,
            error: None,
        },]
    );

    let enrollments: Vec<ExperimentEnrollment> =
        db.collect_all::<ExperimentEnrollment>(StoreId::Enrollments)?;
    assert!(matches!(
        enrollments[0].status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::FeatureConflict {
                conflict_slug: None
            },
            ..
        }
    ));

    Ok(())
}

// In bug 1997373, we added a `prev_gecko_pref_states` field to the EnrollmentStatus schema.
// This test check tht the data deserializes correctly both with and without the new field.
#[cfg(feature = "stateful")]
#[test]
fn test_experiment_schema_with_previous_states() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let prev_gecko_pref_states_empty: EnrollmentStatus = serde_json::from_value(json!({
            "Enrolled": {
            "reason": "Qualified",
            "branch": "some_branch",
        }
    }))
    .unwrap();
    assert!(
        matches!(prev_gecko_pref_states_empty, EnrollmentStatus::Enrolled {ref prev_gecko_pref_states, ..} if prev_gecko_pref_states.is_none())
    );

    let prev_gecko_pref_state_exists: EnrollmentStatus = serde_json::from_value(json!({
    "Enrolled": {
        "reason": "Qualified",
        "branch": "some_branch",
        "prev_gecko_pref_states": [
        {
            "original_value": {
            "pref": "some_pref",
            "branch": "default",
            "value": 5
            },
            "feature_id": "some_control",
            "variable": "some_variable"
        },
        {
            "original_value": {
            "pref": "some_pref_2",
            "branch": "user",
            "value": "hello"
            },
            "feature_id": "some_control_2",
            "variable": "some_variable"
        },
        ]
    }
    }))
    .unwrap();
    assert!(matches!(
            prev_gecko_pref_state_exists,
            EnrollmentStatus::Enrolled {
                prev_gecko_pref_states: Some(ref states),
                ..
            } if states[0].original_value.pref == "some_pref" &&  states[0].original_value.value.clone().unwrap() == 5
            && states[0].feature_id == "some_control" &&  states[0].variable == "some_variable"
            && states[1].original_value.pref == "some_pref_2" &&  states[1].original_value.value.clone().unwrap() == "hello"
    ));
}
