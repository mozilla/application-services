/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    enrollment::{ExperimentEnrollment, PreviousGeckoPrefState},
    error::Result,
    json::PrefValue,
    stateful::gecko_prefs::{
        create_feature_prop_pref_map, GeckoPrefHandler, GeckoPrefState, GeckoPrefStore,
        GeckoPrefStoreState, PrefBranch, PrefEnrollmentData,
    },
    tests::helpers::{get_multi_feature_experiment, TestGeckoPrefHandler},
    EnrolledExperiment,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_gecko_pref_store_map_gecko_prefs_to_enrollment_slugs_and_update_store() -> Result<()> {
    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));
    let handler: Arc<Box<dyn GeckoPrefHandler>> = Arc::new(Box::new(handler));
    let store = GeckoPrefStore::new(handler.clone());
    store.initialize()?;

    let experiment_slug = "slug-1";
    let experiment = get_multi_feature_experiment(
        experiment_slug,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-value"
            }),
        )],
    );
    let experiments = vec![experiment.clone()];
    let experiment_enrollments = vec![ExperimentEnrollment::enrolled(experiment_slug)];
    let experiments_by_slug = HashMap::from_iter([(
        experiment_slug.into(),
        EnrolledExperiment {
            feature_ids: experiment.feature_ids.clone(),
            slug: experiment_slug.into(),
            user_facing_name: "".to_string(),
            user_facing_description: "".to_string(),
            branch_slug: experiment.branches[0].clone().slug,
        },
    )]);

    store.map_gecko_prefs_to_enrollment_slugs_and_update_store(
        &experiments,
        &experiment_enrollments,
        &experiments_by_slug,
    );

    let handler = unsafe {
        std::mem::transmute::<Arc<Box<dyn GeckoPrefHandler>>, Arc<Box<TestGeckoPrefHandler>>>(
            handler,
        )
    };
    let handler_state = handler
        .state
        .lock()
        .expect("Unable to lock transmuted handler state");
    let prefs = handler_state.prefs_set.clone().unwrap();

    assert_eq!(1, prefs.len());
    assert_eq!(prefs[0].gecko_pref.pref, pref_state.gecko_pref.pref);
    assert_eq!(prefs[0].gecko_value, Some(PrefValue::Null));
    assert_eq!(
        prefs[0].enrollment_value.clone().unwrap().pref_value,
        PrefValue::String("some-value".to_string())
    );

    Ok(())
}

#[test]
fn test_gecko_pref_store_map_gecko_prefs_to_enrollment_slugs_and_update_store_experiment_overwrites_rollout(
) -> Result<()> {
    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![(
        "test_feature",
        "test_prop",
        pref_state.clone(),
    )]));
    let handler: Arc<Box<dyn GeckoPrefHandler>> = Arc::new(Box::new(handler));
    let store = GeckoPrefStore::new(handler.clone());
    store.initialize()?;

    let rollout_slug = "rollout-1";
    let mut rollout = get_multi_feature_experiment(
        rollout_slug,
        vec![(
            "test_feature",
            json!({
                "test_prop": "some-rollout-value"
            }),
        )],
    );
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
    );
    let experiments = vec![rollout.clone(), experiment.clone()];
    let experiment_enrollments = vec![
        ExperimentEnrollment::enrolled(rollout_slug),
        ExperimentEnrollment::enrolled(experiment_slug),
    ];
    let experiments_by_slug = HashMap::from_iter([
        (
            rollout_slug.into(),
            EnrolledExperiment {
                feature_ids: rollout.feature_ids.clone(),
                slug: rollout_slug.into(),
                user_facing_name: "".to_string(),
                user_facing_description: "".to_string(),
                branch_slug: rollout.branches[0].clone().slug,
            },
        ),
        (
            experiment_slug.into(),
            EnrolledExperiment {
                feature_ids: experiment.feature_ids.clone(),
                slug: experiment_slug.into(),
                user_facing_name: "".to_string(),
                user_facing_description: "".to_string(),
                branch_slug: experiment.branches[0].clone().slug,
            },
        ),
    ]);

    store.map_gecko_prefs_to_enrollment_slugs_and_update_store(
        &experiments,
        &experiment_enrollments,
        &experiments_by_slug,
    );

    let handler = unsafe {
        std::mem::transmute::<Arc<Box<dyn GeckoPrefHandler>>, Arc<Box<TestGeckoPrefHandler>>>(
            handler,
        )
    };
    let handler_state = handler
        .state
        .lock()
        .expect("Unable to lock transmuted handler state");
    let prefs = handler_state.prefs_set.clone().unwrap();

    assert_eq!(1, prefs.len());
    assert_eq!(prefs[0].gecko_pref.pref, pref_state.gecko_pref.pref);
    assert_eq!(prefs[0].gecko_value, Some(PrefValue::Null));
    assert_eq!(
        prefs[0].enrollment_value.clone().unwrap().pref_value,
        PrefValue::String("some-experiment-value".to_string())
    );

    Ok(())
}

#[test]
fn test_gecko_pref_store_state_update_pref_state() -> Result<()> {
    let pref_state = GeckoPrefState::new("test.pref", None).with_gecko_value(PrefValue::Null);

    let mut pref_store_state = GeckoPrefStoreState {
        gecko_prefs_with_state: create_feature_prop_pref_map(vec![(
            "test_feature",
            "test_prop",
            pref_state.clone(),
        )]),
    };

    let new_pref_state = GeckoPrefState::new("test.pref", None)
        .with_gecko_value(PrefValue::String("some-value".into()));

    pref_store_state.update_pref_state(&new_pref_state);

    assert_eq!(
        pref_store_state
            .gecko_prefs_with_state
            .get("test_feature")
            .unwrap()
            .get("test_prop")
            .unwrap()
            .gecko_value,
        Some(PrefValue::String("some-value".to_string()))
    );

    Ok(())
}

#[test]
fn test_gecko_pref_store_pref_is_user_set() -> Result<()> {
    let pref_state_1 = GeckoPrefState::new("test.pref.set_by_user", None).set_by_user();
    let pref_state_2 = GeckoPrefState::new("test.pref.NOT.set_by_user", None);
    let handler = TestGeckoPrefHandler::new(create_feature_prop_pref_map(vec![
        ("test_feature", "test_prop_1", pref_state_1.clone()),
        ("test_feature", "test_prop_2", pref_state_2.clone()),
    ]));
    let handler: Arc<Box<dyn GeckoPrefHandler>> = Arc::new(Box::new(handler));
    let store = GeckoPrefStore::new(handler.clone());
    store.initialize()?;

    assert!(store.pref_is_user_set("test.pref.set_by_user"));
    assert!(!store.pref_is_user_set("test.pref.NOT.set_by_user"));

    Ok(())
}

#[test]
fn test_build_previous_states() -> Result<()> {
    let pref_state_1 = GeckoPrefState::new("test.some.pref.1", Some(PrefBranch::Default))
        .with_gecko_value(json!("gecko-pref-value-1"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: "experiment-slug-1".to_string(),
            pref_value: json!("pref-value-1"),
            feature_id: "feature-id-1".to_string(),
            variable: "variable-1".to_string(),
        });

    // Belongs to the same experiment variable as 1, but a different pref
    let pref_state_2 = GeckoPrefState::new("test.some.pref.2", Some(PrefBranch::User))
        .with_gecko_value(json!("gecko-pref-value-2"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: "experiment-slug-1".to_string(),
            pref_value: json!("pref-value-2"),
            feature_id: "feature-id-1".to_string(),
            variable: "variable-1".to_string(),
        });

    // Other random independent experiment
    let pref_state_3 = GeckoPrefState::new("test.some.pref.3", Some(PrefBranch::Default))
        .with_gecko_value(json!("gecko-pref-value-3"))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: "experiment-slug-3".to_string(),
            pref_value: json!("experiment-pref-value-3"),
            feature_id: "feature-id-3".to_string(),
            variable: "variable-3".to_string(),
        });

    // Experiment missing gecko value
    let pref_state_4 = GeckoPrefState::new("test.some.pref.4", Some(PrefBranch::Default))
        .with_enrollment_value(PrefEnrollmentData {
            experiment_slug: "experiment-slug-4".to_string(),
            pref_value: json!("experiment-pref-value-4"),
            feature_id: "feature-id-4".to_string(),
            variable: "variable-4".to_string(),
        });

    // Experiment missing enrollment data
    let pref_state_5 = GeckoPrefState::new("test.some.pref.5", Some(PrefBranch::Default))
        .with_gecko_value(json!("gecko-pref-value-5"));

    let pref_states = vec![
        pref_state_1,
        pref_state_2,
        pref_state_3,
        pref_state_4,
        pref_state_5,
    ];
    let previous_states = crate::stateful::gecko_prefs::build_previous_states(&pref_states);

    assert_eq!(3, previous_states.len());
    assert!(previous_states.contains_key("experiment-slug-1"));
    assert!(previous_states.contains_key("experiment-slug-3"));

    let PreviousGeckoPrefState {
        original_values: original_values_1,
        feature_id: feature_id_1,
        variable: variable_1,
    } = previous_states
        .get("experiment-slug-1")
        .expect("Missing slug");

    assert_eq!("feature-id-1", feature_id_1);
    assert_eq!("variable-1", variable_1);
    assert_eq!(2, original_values_1.len());

    assert_eq!("test.some.pref.1", original_values_1[0].pref);
    assert_eq!(PrefBranch::Default, original_values_1[0].branch);
    assert_eq!(
        "gecko-pref-value-1",
        original_values_1[0].value.clone().unwrap()
    );

    assert_eq!("test.some.pref.2", original_values_1[1].pref);
    assert_eq!(PrefBranch::User, original_values_1[1].branch);
    assert_eq!(
        "gecko-pref-value-2",
        original_values_1[1].value.clone().unwrap()
    );

    let PreviousGeckoPrefState {
        original_values: original_values_3,
        feature_id: feature_id_3,
        variable: variable_3,
    } = previous_states
        .get("experiment-slug-3")
        .expect("Missing slug");

    assert_eq!("feature-id-3", feature_id_3);
    assert_eq!("variable-3", variable_3);
    assert_eq!(1, original_values_3.len());
    assert_eq!("test.some.pref.3", original_values_3[0].pref);
    assert_eq!(PrefBranch::Default, original_values_3[0].branch);
    assert_eq!(
        "gecko-pref-value-3",
        original_values_3[0].value.clone().unwrap()
    );

    let PreviousGeckoPrefState {
        original_values: original_values_4,
        feature_id: feature_id_4,
        variable: variable_4,
    } = previous_states
        .get("experiment-slug-4")
        .expect("Missing slug");

    assert_eq!("feature-id-4", feature_id_4);
    assert_eq!("variable-4", variable_4);
    assert_eq!(1, original_values_4.len());
    assert_eq!("test.some.pref.4", original_values_4[0].pref);
    assert_eq!(PrefBranch::Default, original_values_4[0].branch);
    assert_eq!(None, original_values_4[0].value.clone());

    Ok(())
}
