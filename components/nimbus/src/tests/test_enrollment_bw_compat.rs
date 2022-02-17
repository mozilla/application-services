// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::*;
use serde_json::json;

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
fn test_not_enrolled_reason_schema_with_feature_conflict() {
    // ⚠️ Warning : Do not change the JSON data used by this test. ⚠️
    let non_enrollment: ExperimentEnrollment = serde_json::from_value(json!({
        "slug": "secure-gold",
        "status": {"NotEnrolled": {
            "reason": "FeatureConflict",
        }}
    }))
    .unwrap();
    assert!(
        matches!(non_enrollment.status, EnrollmentStatus::NotEnrolled{ ref reason, ..} if reason == &NotEnrolledReason::FeatureConflict)
    );
}
