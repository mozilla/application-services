// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::enrollment::{
    DisqualifiedReason, EnrolledReason, EnrollmentChangeEventType, EnrollmentsEvolver,
    ExperimentEnrollment, NotEnrolledReason, Participation, map_features_by_feature_id,
};
use crate::evaluator::evaluate_enrollment;
use crate::stateful::behavior::EventStore;
use crate::tests::helpers::{get_single_feature_experiment, no_coenrolling_features};
use crate::{
    AppContext, AvailableRandomizationUnits, NimbusTargetingHelper, Result, TargetingAttributes,
};
use crate::{EnrollmentStatus, Experiment};

fn get_localization_bundle() -> Value {
    json!({
        "en": {
            "text-id": "English text",
            "label-id": "English label"
        },
        "fr": {
            "text-id": "French text",
            "label-id": "French label"
        }
    })
}

fn get_l10n_experiment(localizations: Value) -> Experiment {
    let mut exp = get_single_feature_experiment(
        "l10n-experiment",
        "feature-id",
        json!({
            "title": "test",
            "text": {
                "$l10n": {
                    "id": "text-id",
                    "text": "test text",
                    "comment": "test comment 1"
                }
            },
            "items": [
                {
                    "label": {
                        "$l10n": {
                            "id": "label-id",
                            "text": "Label",
                            "comment": "test comment 2"
                        }
                    }
                }
            ],

        }),
    );
    exp.localizations = serde_json::from_value(localizations).unwrap();
    exp
}

fn get_targeting_helper(locale: Option<&str>) -> NimbusTargetingHelper {
    let targeting_attributes: TargetingAttributes = AppContext {
        app_name: "fenix".to_string(),
        app_id: "org.mozilla.fenix".to_string(),
        channel: "nightly".to_string(),
        locale: locale.map(|l| l.to_string()),
        ..Default::default()
    }
    .into();
    NimbusTargetingHelper::new(
        targeting_attributes,
        Arc::new(Mutex::new(EventStore::new())),
        None,
    )
}

#[test]
fn test_l10n_enrolls_when_language_present() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());
    let targeting_helper = get_targeting_helper(Some("fr-CA"));
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &targeting_helper,
    )?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));
    Ok(())
}

#[test]
fn test_l10n_when_language_missing() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());

    let targeting_helper = get_targeting_helper(Some("de-DE"));
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &targeting_helper,
    )?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::L10nMissingLanguage
        }
    ));

    let targeting_helper = get_targeting_helper(None);
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &targeting_helper,
    )?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::L10nNoClientLanguage
        }
    ));

    Ok(())
}

#[test]
fn test_l10n_when_entry_missing() -> Result<()> {
    let experiment_1 = get_l10n_experiment(json!({
        "fr": {
            "label-id": "French label"
        }
    }));
    let experiment_2 = get_l10n_experiment(json!({
        "fr": {
            "text-id": "French text"
        }
    }));
    let targeting_helper = get_targeting_helper(Some("fr-CA"));
    for experiment in [experiment_1, experiment_2] {
        let enrollment = evaluate_enrollment(
            &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
            &experiment,
            &targeting_helper,
        )?;
        assert!(matches!(
            enrollment.status,
            EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::L10nMissingEntry
            }
        ));
    }
    Ok(())
}

#[test]
fn test_no_l10n_unaffected() -> Result<()> {
    let experiment = get_single_feature_experiment("no-l10n-experiment", "feature-id", json!({}));
    let targeting_helper = get_targeting_helper(Some("fr-CA"));
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &targeting_helper,
    )?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));
    Ok(())
}

#[test]
fn test_l10n_substitution_in_feature_map() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());
    let cases = [
        (
            "fr-CA",
            "fr",
            json!({
                "title": "test",
                "text": "French text",
                "items": [{"label": "French label"}]
            }),
        ),
        (
            "en-CA",
            "en",
            json!({
                "title": "test",
                "text": "English text",
                "items": [{"label": "English label"}]
            }),
        ),
    ];

    for (locale, language, expected) in cases {
        let targeting_helper = get_targeting_helper(Some(locale));
        let enrollment = evaluate_enrollment(
            &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
            &experiment,
            &targeting_helper,
        )?;
        let features = map_features_by_feature_id(
            &[enrollment],
            std::slice::from_ref(&experiment),
            &no_coenrolling_features(),
            Some(language),
        );
        let value = &features["feature-id"].feature.value;
        assert_eq!(Value::Object(value.clone()), expected);
    }
    Ok(())
}

#[test]
fn test_l10n_disqualifies_when_language_no_longer_present() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());
    let mut targeting_helper = get_targeting_helper(Some("fr-CA"));
    let aru = AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4());
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);

    let (enrollments, _) = evolver.evolve_enrollments(
        Participation::default(),
        &[],
        std::slice::from_ref(&experiment),
        &[],
        None,
    )?;
    assert!(matches!(
        enrollments[0].status,
        EnrollmentStatus::Enrolled { .. }
    ));

    let (enrollments_1, events) = evolver.evolve_enrollments(
        Participation::default(),
        std::slice::from_ref(&experiment),
        &[get_l10n_experiment(json!({
            "en": {
                "text-id": "English text",
                "label-id": "English label"
            }
        }))],
        &enrollments,
        None,
    )?;
    let enr_1 = enrollments_1.first().expect("an enrollment");
    assert!(matches!(
        &enr_1.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::L10nMissingLanguage,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].language.as_deref(), Some("fr"));

    let (enrollments_2, events) = evolver.evolve_enrollments(
        Participation::default(),
        std::slice::from_ref(&experiment),
        &[get_l10n_experiment(json!({
            "fr": {
                "label-id": "French label"
            }
        }))],
        &enrollments,
        None,
    )?;
    let enr_2 = enrollments_2.first().unwrap();
    assert!(matches!(
        &enr_2.status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::L10nMissingEntry,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].language.as_deref(), Some("fr"));

    Ok(())
}

#[test]
fn test_l10n_disqualifies_opt_in_enrollment() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());
    let mut targeting_helper = get_targeting_helper(Some("de-DE"));
    let aru = AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4());
    let ids = no_coenrolling_features();
    let mut evolver = EnrollmentsEvolver::new(&aru, &mut targeting_helper, &ids);

    let opt_in = ExperimentEnrollment::from_explicit_opt_in(
        &experiment,
        "control",
        EnrolledReason::OptIn,
        Some("fr"),
        &mut vec![],
    )?;

    let (enrollments, events) = evolver.evolve_enrollments(
        Participation::default(),
        std::slice::from_ref(&experiment),
        std::slice::from_ref(&experiment),
        &[opt_in],
        None,
    )?;
    assert!(matches!(
        &enrollments[0].status,
        EnrollmentStatus::Disqualified {
            reason: DisqualifiedReason::L10nMissingLanguage,
            ..
        }
    ));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].reason.as_deref(), Some("l10n-missing-language"));
    assert_eq!(events[0].language.as_deref(), Some("de"));

    Ok(())
}

#[test]
fn test_l10n_feature_map_omits_unlocalizable_features() -> Result<()> {
    let experiment = get_l10n_experiment(get_localization_bundle());
    let enrollment = evaluate_enrollment(
        &AvailableRandomizationUnits::with_nimbus_id(&uuid::Uuid::new_v4()),
        &experiment,
        &get_targeting_helper(Some("fr-CA")),
    )?;
    for language in [Some("de"), None] {
        let features = map_features_by_feature_id(
            std::slice::from_ref(&enrollment),
            std::slice::from_ref(&experiment),
            &no_coenrolling_features(),
            language,
        );
        assert!(!features.contains_key("feature-id"));
    }
    Ok(())
}

#[test]
fn test_l10n_opt_in_refused_when_not_localizable() -> Result<()> {
    let experiment = get_l10n_experiment(json!({
        "fr": {
            "text-id": "French text"
        }
    }));

    for (language, expected_reason) in [
        (Some("de"), "l10n-missing-language"),
        (Some("fr"), "l10n-missing-entry"),
        (None, "l10n-no-client-language"),
    ] {
        let mut events = vec![];
        let result = ExperimentEnrollment::from_explicit_opt_in(
            &experiment,
            "control",
            EnrolledReason::OptIn,
            language,
            &mut events,
        );
        assert!(result.is_err());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].change, EnrollmentChangeEventType::EnrollFailed);
        assert_eq!(events[0].reason.as_deref(), Some(expected_reason));
        assert_eq!(events[0].language.as_deref(), language);
    }

    let experiment = get_l10n_experiment(get_localization_bundle());
    let enrollment = ExperimentEnrollment::from_explicit_opt_in(
        &experiment,
        "control",
        EnrolledReason::OptIn,
        Some("fr"),
        &mut vec![],
    )?;
    assert!(matches!(
        enrollment.status,
        EnrollmentStatus::Enrolled { .. }
    ));

    Ok(())
}
