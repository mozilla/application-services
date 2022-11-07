/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// cargo test --package nimbus-sdk --lib --all-features -- tests::test_evaluator --nocapture

use chrono::Utc;

use crate::behavior::{
    EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter, SingleIntervalCounter,
};
use crate::enrollment::{EnrolledReason, EnrollmentStatus, NotEnrolledReason};
use crate::evaluator::{choose_branch, targeting};
use crate::{
    error::Result, evaluate_enrollment, is_experiment_available, AppContext,
    AvailableRandomizationUnits, Branch, BucketConfig, Experiment, RandomizationUnit,
    TargetingAttributes,
};

#[test]
fn test_locale_substring() -> Result<()> {
    let expression_statement = "'en' in locale || 'de' in locale";
    let ctx = AppContext {
        locale: Some("de-US".to_string()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);
    Ok(())
}

#[test]
fn test_locale_substring_fails() -> Result<()> {
    let expression_statement = "'en' in locale || 'de' in locale";
    let ctx = AppContext {
        locale: Some("cz-US".to_string()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    let enrollment_status = targeting(expression_statement, &targeting_attributes).unwrap();
    if let EnrollmentStatus::NotEnrolled { reason } = enrollment_status {
        if let NotEnrolledReason::NotTargeted = reason {
            // OK
        } else {
            panic!("Expected to fail on NotTargeted reason, got: {:?}", reason)
        }
    } else {
        panic! {"Expected to fail targeting with NotEnrolled, got: {:?}", enrollment_status}
    }
    Ok(())
}

#[test]
fn test_language_region_from_locale() {
    fn test(locale: &str, language: Option<&str>, region: Option<&str>) {
        let app_context = AppContext {
            locale: Some(locale.to_string()),
            ..Default::default()
        };

        let ta: TargetingAttributes = app_context.into();

        assert_eq!(ta.language, language.map(String::from));
        assert_eq!(ta.region, region.map(String::from));
    }

    test("en-US", Some("en"), Some("US"));
    test("es", Some("es"), None);

    test("nim-BUS", Some("nim"), Some("BUS"));

    // Not sure these are useful.
    test("nim-", Some("nim"), None);
    test("-BUS", None, Some("BUS"));
}

#[test]
fn test_geo_targeting_one_locale() -> Result<()> {
    let expression_statement = "language in ['ro']";
    let ctx = AppContext {
        locale: Some("ro".to_string()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);
    Ok(())
}

#[test]
fn test_geo_targeting_multiple_locales() -> Result<()> {
    let expression_statement = "language in ['en', 'ro']";
    let ctx = AppContext {
        locale: Some("ro".to_string()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);
    Ok(())
}

#[test]
fn test_geo_targeting_fails_properly() -> Result<()> {
    let expression_statement = "language in ['en', 'ro']";
    let ctx = AppContext {
        locale: Some("ar".to_string()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    let enrollment_status = targeting(expression_statement, &targeting_attributes).unwrap();
    if let EnrollmentStatus::NotEnrolled { reason } = enrollment_status {
        if let NotEnrolledReason::NotTargeted = reason {
            // OK
        } else {
            panic!("Expected to fail on NotTargeted reason, got: {:?}", reason)
        }
    } else {
        panic! {"Expected to fail targeting with NotEnrolled, got: {:?}", enrollment_status}
    }
    Ok(())
}

#[test]
fn test_minimum_version_targeting_passes() -> Result<()> {
    // Here's our valid jexl statement
    let expression_statement = "app_version|versionCompare('96.!') >= 0";
    let ctx = AppContext {
        app_version: Some("97pre.1.0-beta.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);
    Ok(())
}

#[test]
fn test_minimum_version_targeting_fails() -> Result<()> {
    // Here's our valid jexl statement
    let expression_statement = "app_version|versionCompare('96+.0') >= 0";
    let ctx = AppContext {
        app_version: Some("96.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    Ok(())
}

#[test]
fn test_targeting_specific_verision() -> Result<()> {
    // Here's our valid jexl statement that targets **only** 96 versions
    let expression_statement =
        "(app_version|versionCompare('96.!') >= 0) && (app_version|versionCompare('97.!') < 0)";
    let ctx = AppContext {
        app_version: Some("96.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    // OK 96.1 is a 96 version
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);
    let ctx = AppContext {
        app_version: Some("97.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    // Not targeted, version is 97
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );

    let ctx = AppContext {
        app_version: Some("95.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();

    // Not targeted, version is 95
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );

    Ok(())
}

#[test]
fn test_targeting_invalid_transform() -> Result<()> {
    let expression_statement = "app_version|invalid_transform('96+.0')";
    let ctx = AppContext {
        app_version: Some("96.1".into()),
        ..Default::default()
    };
    let targeting_attributes = ctx.into();
    let err = targeting(expression_statement, &targeting_attributes);
    if let Some(e) = err {
        if let EnrollmentStatus::Error { reason: _ } = e {
            // OK
        } else {
            panic!("Should have returned an error since the transform doesn't exist")
        }
    } else {
        panic!("Should not have been targeted")
    }
    Ok(())
}

#[test]
fn test_targeting() {
    // Here's our valid jexl statement
    let expression_statement =
        "app_id == '1010' && (app_version|versionCompare('4.0') >= 0 || locale == \"en-US\")";

    // A matching context testing the logical AND + OR of the expression
    let targeting_attributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // A matching context testing the logical OR of the expression
    let targeting_attributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("de-DE".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // A matching context testing the other branch of the logical OR
    let targeting_attributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("3.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // A non-matching context testing the logical AND of the expression
    let non_matching_targeting = AppContext {
        app_name: "not_nimbus_test".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    assert!(matches!(
        targeting(expression_statement, &non_matching_targeting),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    ));

    // A non-matching context testing the logical OR of the expression
    let non_matching_targeting = AppContext {
        app_name: "not_nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("3.5".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("de-DE".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    assert!(matches!(
        targeting(expression_statement, &non_matching_targeting),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    ));
}
use std::collections::{HashMap, HashSet, VecDeque};

#[test]
fn test_targeting_custom_targeting_attributes() {
    // Here's our valid jexl statement
    let expression_statement =
        "app_id == '1010' && (app_version == '4.4' || locale == \"en-US\") && is_first_run == 'true' && ios_version == '8.8'";

    let mut custom_targeting_attributes = HashMap::new();
    custom_targeting_attributes.insert("is_first_run".into(), "true".into());
    custom_targeting_attributes.insert("ios_version".into(), "8.8".into());
    // A matching context that includes the appropriate specific context
    let targeting_attributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: Some(custom_targeting_attributes),
        ..Default::default()
    }
    .into();
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // A matching context without the specific context
    let targeting_attributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    // We haven't defined `is_first_run` here, so this should error out, i.e. return an error.
    assert!(matches!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::Error { .. })
    ));
}

#[test]
fn test_targeting_is_already_enrolled() {
    // Here's our valid jexl statement
    let expression_statement = "is_already_enrolled";
    // A matching context that includes the appropriate specific context
    let mut targeting_attributes: TargetingAttributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    targeting_attributes.is_already_enrolled = true;

    // The targeting should pass!
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // We make the is_already_enrolled false and try again
    targeting_attributes.is_already_enrolled = false;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_targeting_active_experiments_equivalency() {
    // Here's our valid jexl statement
    let expression_statement = "'test' in active_experiments";
    // A matching context that includes the appropriate specific context
    let mut targeting_attributes: TargetingAttributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    let mut set = HashSet::<String>::new();
    set.insert("test".into());
    targeting_attributes.active_experiments = set;

    // The targeting should pass!
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // We set active_experiment treatment to something not expected and try again
    let mut set = HashSet::<String>::new();
    set.insert("test1".into());
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );

    // We set active_experiments to None and try again
    let set = HashSet::<String>::new();
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_targeting_active_experiments_exists() {
    // Here's our valid jexl statement
    let expression_statement = "'test' in active_experiments";
    // A matching context that includes the appropriate specific context
    let mut targeting_attributes: TargetingAttributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    let mut set = HashSet::<String>::new();
    set.insert("test".into());
    targeting_attributes.active_experiments = set;

    // The targeting should pass!
    assert_eq!(targeting(expression_statement, &targeting_attributes), None);

    // We set active_experiment treatment to something not expected and try again
    let set = HashSet::<String>::new();
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_invalid_expression() {
    // This expression doesn't return a bool
    let expression_statement = "2.0";

    assert_eq!(
        targeting(expression_statement, &Default::default()),
        Some(EnrollmentStatus::Error {
            reason: "Invalid Expression - didn't evaluate to a bool".to_string()
        })
    )
}

#[test]
fn test_evaluation_error() {
    // This is an invalid JEXL statement
    let expression_statement = "This is not a valid JEXL expression";

    assert!(
        matches!(targeting(expression_statement, &Default::default()), Some(EnrollmentStatus::Error { reason }) if reason.starts_with("EvaluationError:"))
    )
}

#[test]
fn test_choose_branch() {
    let slug = "TEST_EXP1";
    let branches = vec![
        Branch {
            slug: "control".to_string(),
            ratio: 1,
            feature: None,
            features: None,
        },
        Branch {
            slug: "blue".to_string(),
            ratio: 1,
            feature: None,
            features: None,
        },
    ];
    // 299eed1e-be6d-457d-9e53-da7b1a03f10d maps to the second index
    let id = uuid::Uuid::parse_str("3d2142de-53bf-2d48-a92d-45fb7036cbf6").unwrap();
    let b = choose_branch(slug, &branches, &id.to_string()).unwrap();
    assert_eq!(b.slug, "blue");
    // 542213c0-9aef-47eb-bc6b-3b8529736ba2 maps to the first index
    let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
    let b = choose_branch(slug, &branches, &id.to_string()).unwrap();
    assert_eq!(b.slug, "control");
}

#[test]
fn test_is_experiment_available() {
    let experiment = Experiment {
        app_name: Some("NimbusTest".to_string()),
        app_id: Some("org.example.app".to_string()),
        channel: Some("production".to_string()),
        schema_version: "1.0.0".to_string(),
        slug: "TEST_EXP".to_string(),
        is_enrollment_paused: false,
        feature_ids: vec!["monkey".to_string()],
        bucket_config: BucketConfig {
            randomization_unit: RandomizationUnit::NimbusId,
            start: 0,
            count: 10000,
            total: 10000,
            ..Default::default()
        },
        branches: vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
        ],
        reference_branch: Some("control".to_string()),
        ..Default::default()
    };

    // Application context for matching the above experiment.  If any of the `app_name`, `app_id`,
    // or `channel` doesn't match the experiment, then the client won't be enrolled.
    let app_context = AppContext {
        app_name: "NimbusTest".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };
    // If is_release is true, we should match on the exact combination of
    // app_name, channel and app_id.
    assert!(!is_experiment_available(&app_context, &experiment, true));

    // If is_release is false, we only match on app_name.
    // As a nightly build, we want to be able to test production experiments
    assert!(is_experiment_available(&app_context, &experiment, false));

    let experiment = Experiment {
        channel: Some("nightly".to_string()),
        ..experiment
    };
    // channels now match, so should be availble for enrollment (true) and testing (false)
    assert!(is_experiment_available(&app_context, &experiment, true));
    assert!(is_experiment_available(&app_context, &experiment, false));

    let experiment = Experiment {
        app_name: Some("a_different_app".to_string()),
        ..experiment
    };
    assert!(!is_experiment_available(&app_context, &experiment, false));
    assert!(!is_experiment_available(&app_context, &experiment, false));
}

#[test]
fn test_qualified_enrollment() {
    let experiment = Experiment {
        app_name: Some("NimbusTest".to_string()),
        app_id: Some("org.example.app".to_string()),
        channel: Some("nightly".to_string()),
        schema_version: "1.0.0".to_string(),
        slug: "TEST_EXP".to_string(),
        is_enrollment_paused: false,
        feature_ids: vec!["monkey".to_string()],
        bucket_config: BucketConfig {
            randomization_unit: RandomizationUnit::NimbusId,
            start: 0,
            count: 10000,
            total: 10000,
            ..Default::default()
        },
        branches: vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
        ],
        reference_branch: Some("control".to_string()),
        ..Default::default()
    };

    // Application context for matching the above experiment.  If the `app_name` or
    // `channel` doesn't match the experiment, then the client won't be enrolled.
    let mut targeting_attributes = AppContext {
        app_name: "NimbusTest".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();

    let id = uuid::Uuid::new_v4();

    let enrollment =
        evaluate_enrollment(&id, &Default::default(), &targeting_attributes, &experiment).unwrap();
    println!("Uh oh!  {:#?}", enrollment.status);
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));

    // Change the channel to test when it has a different case than expected
    // (See SDK-246: https://jira.mozilla.com/browse/SDK-246 )
    targeting_attributes.app_context.channel = "Nightly".to_string();

    // Now we will be enrolled in the experiment because we have the right channel, but with different capitalization
    let enrollment =
        evaluate_enrollment(&id, &Default::default(), &targeting_attributes, &experiment).unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));
}

#[test]
fn test_wrong_randomization_units() {
    let experiment = Experiment {
        app_name: Some("NimbusTest".to_string()),
        app_id: Some("org.example.app".to_string()),
        channel: Some("nightly".to_string()),
        schema_version: "1.0.0".to_string(),
        slug: "TEST_EXP".to_string(),
        is_enrollment_paused: false,
        feature_ids: vec!["test-feature".to_string()],
        bucket_config: BucketConfig {
            randomization_unit: RandomizationUnit::ClientId,
            start: 0,
            count: 10000,
            total: 10000,
            ..Default::default()
        },
        branches: vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
        ],
        reference_branch: Some("control".to_string()),
        ..Default::default()
    };

    // Application context for matching the above experiment.  If any of the `app_name`, `app_id`,
    // or `channel` doesn't match the experiment, then the client won't be enrolled.
    let targeting_attributes = AppContext {
        app_name: "NimbusTest".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();

    // We won't be enrolled in the experiment because we don't have the right randomization units since the
    // experiment is requesting the `ClientId` and the `Default::default()` here will just have the
    // NimbusId.
    let enrollment = evaluate_enrollment(
        &uuid::Uuid::new_v4(),
        &Default::default(),
        &targeting_attributes,
        &experiment,
    )
    .unwrap();
    // The status should be `Error`
    assert!(matches!(enrollment.status, EnrollmentStatus::Error { .. }));

    // Fits because of the client_id.
    let available_randomization_units = AvailableRandomizationUnits::with_client_id("bobo");
    let id = uuid::Uuid::parse_str("542213c0-9aef-47eb-bc6b-3b8529736ba2").unwrap();
    let enrollment = evaluate_enrollment(
        &id,
        &available_randomization_units,
        &targeting_attributes,
        &experiment,
    )
    .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));
}

#[test]
fn test_not_targeted_for_enrollment() {
    let experiment = Experiment {
        app_name: Some("NimbusTest".to_string()),
        app_id: Some("org.example.app".to_string()),
        channel: Some("nightly".to_string()),
        schema_version: "1.0.0".to_string(),
        slug: "TEST_EXP2".to_string(),
        is_enrollment_paused: false,
        feature_ids: vec!["test-feature".to_string()],
        bucket_config: BucketConfig {
            randomization_unit: RandomizationUnit::NimbusId,
            start: 0,
            count: 10000,
            total: 10000,
            ..Default::default()
        },
        branches: vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
        ],
        reference_branch: Some("control".to_string()),
        ..Default::default()
    };

    let id = uuid::Uuid::new_v4();

    // If the `app_name` or `channel` doesn't match the experiment,
    // then the client won't be enrolled.
    // Start with a context that does't match the app_name:
    let mut targeting_attributes = AppContext {
        app_name: "Wrong!".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();

    // We won't be enrolled in the experiment because we don't have the right app_name
    let enrollment =
        evaluate_enrollment(&id, &Default::default(), &targeting_attributes, &experiment).unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        }
    ));

    // Change the app_name back and change the channel to test when it doesn't match:
    targeting_attributes.app_context.app_name = "NimbusTest".to_string();
    targeting_attributes.app_context.channel = "Wrong".to_string();

    // Now we won't be enrolled in the experiment because we don't have the right channel, but with the same
    // `NotTargeted` reason
    let enrollment =
        evaluate_enrollment(&id, &Default::default(), &targeting_attributes, &experiment).unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        }
    ));
}

#[test]
fn test_enrollment_bucketing() {
    let experiment = Experiment {
        app_id: Some("org.example.app".to_string()),
        channel: Some("nightly".to_string()),
        schema_version: "1.0.0".to_string(),
        slug: "TEST_EXP1".to_string(),
        is_enrollment_paused: false,
        feature_ids: vec!["test-feature".to_string()],
        bucket_config: BucketConfig {
            randomization_unit: RandomizationUnit::NimbusId,
            namespace:
                "bug-1637316-message-aboutwelcome-pull-factor-reinforcement-76-rel-release-76-77"
                    .to_string(),
            start: 0,
            count: 2000,
            total: 10000,
        },
        branches: vec![
            Branch {
                slug: "control".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
            Branch {
                slug: "blue".to_string(),
                ratio: 1,
                feature: None,
                features: None,
            },
        ],
        reference_branch: Some("control".to_string()),
        ..Default::default()
    };

    let available_randomization_units = Default::default();
    // 299eed1e-be6d-457d-9e53-da7b1a03f10d uuid fits in start: 0, count: 2000, total: 10000 with the example namespace, to the treatment-variation-b branch
    // Tested against the desktop implementation
    let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
    // Application context for matching exp3
    let targeting_attributes = AppContext {
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();

    let enrollment = evaluate_enrollment(
        &id,
        &available_randomization_units,
        &targeting_attributes,
        &experiment,
    )
    .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled {
            reason: EnrolledReason::Qualified,
            ..
        }
    ));
}

#[test]
fn test_events_sum_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter::from(
        IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: VecDeque::from(vec![1, 1, 0]),
        },
        IntervalConfig::new(3, Interval::Days),
    )]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let mut targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();
    targeting_attributes.event_store = Some(event_store);
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 3, 0) > 2",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 3, 0) > 1",
            &targeting_attributes
        ),
        None
    );
}

#[test]
fn test_events_count_non_zero_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter::from(
        IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: VecDeque::from(vec![1, 2, 0]),
        },
        IntervalConfig::new(3, Interval::Days),
    )]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let mut targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();
    targeting_attributes.event_store = Some(event_store);
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsCountNonZero('Days', 3, 0) > 2",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsCountNonZero('Days', 3, 0) > 1",
            &targeting_attributes
        ),
        None
    );
}

#[test]
fn test_events_average_per_interval_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter::from(
        IntervalData {
            bucket_count: 7,
            starting_instant: Utc::now(),
            buckets: VecDeque::from(vec![1, 2, 0, 0, 0, 2, 3]),
        },
        IntervalConfig::new(7, Interval::Days),
    )]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let mut targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();
    targeting_attributes.event_store = Some(event_store);
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsAveragePerInterval('Days', 7, 0) > 2",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsAveragePerInterval('Days', 7, 0) > 1.14",
            &targeting_attributes
        ),
        None
    );
}

#[test]
fn test_events_average_per_non_zero_interval_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter::from(
        IntervalData {
            bucket_count: 7,
            starting_instant: Utc::now(),
            buckets: VecDeque::from(vec![1, 2, 0, 0, 0, 2, 4]),
        },
        IntervalConfig::new(7, Interval::Days),
    )]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let mut targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();
    targeting_attributes.event_store = Some(event_store);
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsAveragePerNonZeroInterval('Days', 7, 0) == 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsAveragePerNonZeroInterval('Days', 7, 0) == 2.25",
            &targeting_attributes
        ),
        None
    );
}

#[test]
fn test_events_transforms_parameters() {
    let targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();

    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 3) > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JexlError: Transform parameter error: events transforms require 3 parameters"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "1|eventsSum('Days', 3, 0) > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum(1, 3, 0) > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Day', 3, 0) > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Behavior error: IntervalParseError: Day is not a valid Interval"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 'test', 0) > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JexlError: Transform parameter error: events transforms require a positive number as the second parameter"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 3, 'test') > 1",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JexlError: Transform parameter error: events transforms require a positive number as the third parameter"
                .to_string()
        })
    );
}

#[test]
fn test_events_transforms_missing_event_store() {
    let targeting_attributes: TargetingAttributes = AppContext {
        ..Default::default()
    }
    .into();

    assert_eq!(
        targeting(
            "'app.foregrounded'|eventsSum('Days', 3, 0) > 2",
            &targeting_attributes
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Behavior error: The event store is not available on the targeting attributes"
                .to_string()
        })
    );
}
