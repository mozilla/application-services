/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */
#![cfg_attr(not(feature = "stateful"), allow(clippy::needless_update))]

use crate::{
    enrollment::{EnrolledReason, EnrollmentStatus, NotEnrolledReason},
    evaluate_enrollment,
    evaluator::{choose_branch, is_experiment_available, targeting},
    AppContext, AvailableRandomizationUnits, Branch, BucketConfig, Experiment, RandomizationUnit,
    Result, TargetingAttributes,
};
use serde_json::{json, Map, Value};

pub fn ta_with_locale(locale: String) -> TargetingAttributes {
    let app_ctx = AppContext {
        #[cfg(feature = "stateful")]
        locale: Some(locale),
        ..Default::default()
    };
    #[cfg(not(feature = "stateful"))]
    let req_ctx = Map::from_iter([("locale".to_string(), Value::String(locale))]);

    cfg_if::cfg_if! {
        if #[cfg(feature = "stateful")] {
            app_ctx.into()
        } else {
            TargetingAttributes::new(app_ctx, req_ctx)
        }
    }
}

#[test]
fn test_locale_substring() -> Result<()> {
    let expression_statement = "'en' in locale || 'de' in locale";
    let ta = ta_with_locale("de-US".to_string());

    assert_eq!(targeting(expression_statement, &ta.into()), None);
    Ok(())
}

#[test]
fn test_locale_substring_fails() -> Result<()> {
    let expression_statement = "'en' in locale || 'de' in locale";
    let ta = ta_with_locale("cz-US".to_string());
    let enrollment_status = targeting(expression_statement, &ta.into()).unwrap();
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
        let ta = ta_with_locale(locale.to_string());

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
    let ta = ta_with_locale("ro".to_string());

    assert_eq!(targeting(expression_statement, &ta.into()), None);
    Ok(())
}

#[test]
fn test_geo_targeting_multiple_locales() -> Result<()> {
    let expression_statement = "language in ['en', 'ro']";
    let ta = ta_with_locale("ro".to_string());
    assert_eq!(targeting(expression_statement, &ta.into()), None);
    Ok(())
}

#[test]
fn test_geo_targeting_fails_properly() -> Result<()> {
    let expression_statement = "language in ['en', 'ro']";
    let ta = ta_with_locale("ar".to_string());
    let enrollment_status = targeting(expression_statement, &ta.into()).unwrap();
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

#[cfg(feature = "stateful")]
#[test]
fn test_minimum_version_targeting_passes() -> Result<()> {
    // Here's our valid jexl statement
    let expression_statement = "app_version|versionCompare('96.!') >= 0";
    let ctx = AppContext {
        app_version: Some("97pre.1.0-beta.1".into()),
        ..Default::default()
    };
    assert_eq!(targeting(expression_statement, &ctx.into()), None);
    Ok(())
}

#[cfg(feature = "stateful")]
#[test]
fn test_minimum_version_targeting_fails() -> Result<()> {
    // Here's our valid jexl statement
    let expression_statement = "app_version|versionCompare('96+.0') >= 0";
    let ctx = AppContext {
        app_version: Some("96.1".into()),
        ..Default::default()
    };
    assert_eq!(
        targeting(expression_statement, &ctx.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    Ok(())
}

#[cfg(feature = "stateful")]
#[test]
fn test_targeting_specific_version() -> Result<()> {
    // Here's our valid jexl statement that targets **only** 96 versions
    let expression_statement =
        "(app_version|versionCompare('96.!') >= 0) && (app_version|versionCompare('97.!') < 0)";
    let ctx = AppContext {
        app_version: Some("96.1".into()),
        ..Default::default()
    };
    // OK 96.1 is a 96 version
    assert_eq!(targeting(expression_statement, &ctx.into()), None);
    let ctx = AppContext {
        app_version: Some("97.1".into()),
        ..Default::default()
    };
    // Not targeted, version is 97
    assert_eq!(
        targeting(expression_statement, &ctx.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );

    let ctx = AppContext {
        app_version: Some("95.1".into()),
        ..Default::default()
    };
    // Not targeted, version is 95
    assert_eq!(
        targeting(expression_statement, &ctx.into()),
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
    let err = targeting(expression_statement, &ctx.into());
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

#[cfg(feature = "stateful")]
#[test]
fn test_targeting() {
    // Here's our valid jexl statement
    let expression_statement =
        "app_id == '1010' && (app_version|versionCompare('4.0') >= 0 || app_build == \"1234\")";

    // A matching context testing the logical AND + OR of the expression
    let ctx = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    assert_eq!(targeting(expression_statement, &ctx.into()), None);

    // A matching context testing the logical OR of the expression
    let ctx = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    assert_eq!(targeting(expression_statement, &ctx.into()), None);

    // A matching context testing the other branch of the logical OR
    let ctx = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("3.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    assert_eq!(targeting(expression_statement, &ctx.into()), None);

    // A non-matching context testing the logical AND of the expression
    let ctx = AppContext {
        app_name: "not_nimbus_test".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    assert!(matches!(
        targeting(expression_statement, &ctx.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    ));

    // A non-matching context testing the logical OR of the expression
    let ctx = AppContext {
        app_name: "not_nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("3.5".to_string()),
        app_build: Some("12345".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    assert!(matches!(
        targeting(expression_statement, &ctx.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    ));
}

#[test]
fn test_targeting_custom_targeting_attributes() {
    // Here's our valid jexl statement
    let expression_statement =
        "app_id == '1010' && (app_version == '4.4' || app_build == \"1234\") && is_first_run == true && ios_version == '8.8'";

    let mut custom_targeting_attributes = Map::<String, Value>::new();
    custom_targeting_attributes.insert("is_first_run".into(), json!(true));
    custom_targeting_attributes.insert("ios_version".into(), json!("8.8"));
    // A matching context that includes the appropriate specific context
    let ctx = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: Some(custom_targeting_attributes),
        ..Default::default()
    };
    assert_eq!(targeting(expression_statement, &ctx.into()), None);

    // A matching context without the specific context
    let ctx = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    // We haven't defined `is_first_run` here, so this should error out, i.e. return an error.
    assert!(matches!(
        targeting(expression_statement, &ctx.into()),
        Some(EnrollmentStatus::Error { .. })
    ));
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

    assert!(matches!(
            targeting(expression_statement, &Default::default()),
            Some(EnrollmentStatus::Error { reason }) if reason.starts_with("EvaluationError:")))
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
    let th = AppContext {
        app_name: "NimbusTest".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    }
    .into();
    // If is_release is true, we should match on the exact combination of
    // app_name, channel and app_id.
    assert!(!is_experiment_available(&th, &experiment, true));

    // If is_release is false, we only match on app_name.
    // As a nightly build, we want to be able to test production experiments
    assert!(is_experiment_available(&th, &experiment, false));

    let experiment = Experiment {
        channel: Some("nightly".to_string()),
        ..experiment
    };
    // channels now match, so should be available for enrollment (true) and testing (false)
    assert!(is_experiment_available(&th, &experiment, true));
    assert!(is_experiment_available(&th, &experiment, false));

    let experiment = Experiment {
        app_name: Some("a_different_app".to_string()),
        ..experiment
    };
    assert!(!is_experiment_available(&th, &experiment, false));
    assert!(!is_experiment_available(&th, &experiment, false));
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
    let mut ctx = AppContext {
        app_name: "NimbusTest".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let id = uuid::Uuid::new_v4();

    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&id),
        &experiment,
        &ctx.clone().into(),
    )
    .unwrap();
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
    ctx.channel = "Nightly".to_string();

    // Now we will be enrolled in the experiment because we have the right channel, but with different capitalization
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&id),
        &experiment,
        &ctx.into(),
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
            randomization_unit: RandomizationUnit::UserId,
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
    let ctx = AppContext {
        app_name: "NimbusTest".to_string(),
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    // We won't be enrolled in the experiment because we don't have the right randomization units since the
    // experiment is requesting the `UserId` and the `Default::default()` here will just have the
    // NimbusId.
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &ctx.clone().into(),
    )
    .unwrap();
    // The status should be `Error`
    assert!(matches!(enrollment.status, EnrollmentStatus::Error { .. }));

    // Fits because of the user_id.
    let available_randomization_units = AvailableRandomizationUnits::with_user_id("bobo");
    let enrollment =
        evaluate_enrollment(&available_randomization_units, &experiment, &ctx.into()).unwrap();
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
    let mut ctx = AppContext {
        app_name: "Wrong!".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    // We won't be enrolled in the experiment because we don't have the right app_name
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&id),
        &experiment,
        &ctx.clone().into(),
    )
    .unwrap();
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        }
    ));

    // Change the app_name back and change the channel to test when it doesn't match:
    ctx.app_name = "NimbusTest".to_string();
    ctx.channel = "Wrong".to_string();

    // Now we won't be enrolled in the experiment because we don't have the right channel, but with the same
    // `NotTargeted` reason
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&id),
        &experiment,
        &ctx.into(),
    )
    .unwrap();
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

    let available_randomization_units: AvailableRandomizationUnits = Default::default();
    // 299eed1e-be6d-457d-9e53-da7b1a03f10d uuid fits in start: 0, count: 2000, total: 10000 with the example namespace, to the treatment-variation-b branch
    // Tested against the desktop implementation
    let id = uuid::Uuid::parse_str("299eed1e-be6d-457d-9e53-da7b1a03f10d").unwrap();
    // Application context for matching exp3
    let ctx = AppContext {
        app_id: "org.example.app".to_string(),
        channel: "nightly".to_string(),
        ..Default::default()
    };

    let enrollment = evaluate_enrollment(
        &available_randomization_units.apply_nimbus_id(&id),
        &experiment,
        &ctx.into(),
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

#[cfg(not(feature = "stateful"))]
#[test]
fn test_lang_region_overrides() {
    let request = json!({
        "language": "en",
        "region": "US",
    });
    let ta = TargetingAttributes::new(AppContext::default(), request.as_object().unwrap().clone());
    let value = serde_json::to_value(ta).unwrap();
    assert_eq!(value.get("language").unwrap(), &json!("en"));
    assert_eq!(value.get("region").unwrap(), &json!("US"));

    let request = json!({
        "locale": "en",
        "region": "US",
    });
    let ta = TargetingAttributes::new(AppContext::default(), request.as_object().unwrap().clone());
    let value = serde_json::to_value(ta).unwrap();
    assert_eq!(value.get("language").unwrap(), &json!("en"));
    assert_eq!(value.get("region").unwrap(), &json!("US"));

    let request = json!({
        "locale": "es-CX",
        "language": "en",
        "region": "US",
    });
    let ta = TargetingAttributes::new(AppContext::default(), request.as_object().unwrap().clone());
    let value = serde_json::to_value(ta).unwrap();
    assert_eq!(value.get("language").unwrap(), &json!("en"));
    assert_eq!(value.get("region").unwrap(), &json!("US"));
}
