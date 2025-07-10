/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use crate::{
    enrollment::{EnrollmentStatus, ExperimentEnrollment},
    error::Result,
    json::PrefValue,
    EnrolledExperiment, Experiment, NimbusError,
};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PrefBranch {
    Default,
    User,
}

impl Display for PrefBranch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PrefBranch::Default => f.write_str("default"),
            PrefBranch::User => f.write_str("user"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeckoPref {
    pub pref: String,
    pub branch: PrefBranch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefEnrollmentData {
    pub pref_value: PrefValue,
    pub feature_id: String,
    pub variable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeckoPrefState {
    pub gecko_pref: GeckoPref,
    pub gecko_value: Option<PrefValue>,
    pub enrollment_value: Option<PrefEnrollmentData>,
    pub is_user_set: bool,
}

impl GeckoPrefState {
    pub fn new(pref: &str, branch: Option<PrefBranch>) -> Self {
        Self {
            gecko_pref: GeckoPref {
                pref: pref.into(),
                branch: branch.unwrap_or(PrefBranch::Default),
            },
            gecko_value: None,
            enrollment_value: None,
            is_user_set: false,
        }
    }

    pub fn with_gecko_value(mut self, value: PrefValue) -> Self {
        self.gecko_value = Some(value);
        self
    }

    pub fn with_enrollment_value(mut self, pref_enrollment_data: PrefEnrollmentData) -> Self {
        self.enrollment_value = Some(pref_enrollment_data);
        self
    }

    pub fn set_by_user(mut self) -> Self {
        self.is_user_set = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Copy)]
pub enum PrefUnenrollReason {
    Changed,
    FailedToSet,
}

pub type MapOfFeatureIdToPropertyNameToGeckoPrefState =
    HashMap<String, HashMap<String, GeckoPrefState>>;

pub fn create_feature_prop_pref_map(
    list: Vec<(&str, &str, GeckoPrefState)>,
) -> MapOfFeatureIdToPropertyNameToGeckoPrefState {
    list.iter().fold(
        HashMap::new(),
        |mut feature_map, (feature_id, prop_name, pref_state)| {
            feature_map
                .entry(feature_id.to_string())
                .or_default()
                .insert(prop_name.to_string(), pref_state.clone());
            feature_map
        },
    )
}

pub trait GeckoPrefHandler: Send + Sync {
    /// Used to obtain the prefs values from Gecko
    fn get_prefs_with_state(&self) -> MapOfFeatureIdToPropertyNameToGeckoPrefState;

    /// Used to set the state for each pref based on enrollments
    fn set_gecko_prefs_state(&self, new_prefs_state: Vec<GeckoPrefState>);
}

#[derive(Default)]
pub struct GeckoPrefStoreState {
    pub gecko_prefs_with_state: MapOfFeatureIdToPropertyNameToGeckoPrefState,
}

impl GeckoPrefStoreState {
    pub fn update_pref_state(&mut self, new_pref_state: &GeckoPrefState) -> bool {
        self.gecko_prefs_with_state
            .iter_mut()
            .find_map(|(_, props)| {
                props.iter_mut().find_map(|(_, pref_state)| {
                    if pref_state.gecko_pref.pref == new_pref_state.gecko_pref.pref {
                        *pref_state = new_pref_state.clone();
                        Some(true)
                    } else {
                        None
                    }
                })
            })
            .is_some()
    }
}

pub struct GeckoPrefStore {
    // This is Arc<Box<_>> because of FFI
    pub handler: Arc<Box<dyn GeckoPrefHandler>>,
    pub state: Mutex<GeckoPrefStoreState>,
}

impl GeckoPrefStore {
    pub fn new(handler: Arc<Box<dyn GeckoPrefHandler>>) -> Self {
        Self {
            handler,
            state: Mutex::new(GeckoPrefStoreState::default()),
        }
    }

    pub fn initialize(&self) -> Result<()> {
        let prefs = self.handler.get_prefs_with_state();
        let mut state = self
            .state
            .lock()
            .expect("Unable to lock GeckoPrefStore state");
        state.gecko_prefs_with_state = prefs;

        Ok(())
    }

    pub fn get_mutable_pref_state(&self) -> MutexGuard<GeckoPrefStoreState> {
        self.state
            .lock()
            .expect("Unable to lock GeckoPrefStore state")
    }

    pub fn pref_is_user_set(&self, pref: &str) -> bool {
        let state = self.get_mutable_pref_state();
        state
            .gecko_prefs_with_state
            .iter()
            .find_map(|(_, props)| {
                props.iter().find_map(|(_, gecko_pref_state)| {
                    if gecko_pref_state.gecko_pref.pref == pref {
                        Some(gecko_pref_state.is_user_set)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false)
    }

    /// This method accomplishes a number of tasks important to the Gecko pref enrollment workflow.
    /// 1. It returns a map of pref string to a vector of enrolled recipes in which the value for
    ///    the enrolled branch's feature values includes the property of that feature that sets the
    ///    aforementioned pref.
    /// 2. It updates the GeckoPrefStore state, such that the appropriate GeckoPrefState's
    ///    `enrollment_value` reflects the appropriate value.
    pub fn map_gecko_prefs_to_enrollment_slugs_and_update_store(
        &self,
        // contains full experiment metadata
        experiments: &[Experiment],
        // contains enrollment status for a given experiment
        enrollments: &[ExperimentEnrollment],
        // contains slug of enrolled branch
        experiments_by_slug: &HashMap<String, EnrolledExperiment>,
    ) -> HashMap<String, HashSet<String>> {
        struct RecipeData<'a> {
            experiment: &'a Experiment,
            experiment_enrollment: &'a ExperimentEnrollment,
            branch_slug: &'a str,
        }

        let mut state = self.get_mutable_pref_state();

        /* List of tuples that contain recipe slug, rollout bool, list of feature ids, and
         * branch, in that order.
         */
        let mut recipe_data: Vec<RecipeData> = vec![];

        for experiment_enrollment in enrollments {
            let experiment = match experiments
                .iter()
                .find(|experiment| experiment.slug == experiment_enrollment.slug)
            {
                Some(exp) => exp,
                None => continue,
            };
            recipe_data.push(RecipeData {
                experiment,
                experiment_enrollment,
                branch_slug: match experiments_by_slug.get(&experiment.slug) {
                    Some(ee) => &ee.branch_slug,
                    None => continue,
                },
            });
        }
        // sort `recipe_data` such that rollouts are applied before experiments
        recipe_data.sort_by(
            |a, b| match (a.experiment.is_rollout, b.experiment.is_rollout) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => Ordering::Equal,
            },
        );

        /* This map will ultimately be returned from the function, as a map of pref strings to
         * relevant enrolled recipe slugs, 'relevant' meaning experiments whose enrolled branch
         * values apply a value to a prop for which there is a Gecko pref.
         *
         * We start by iterating mutably over the map of features to props to gecko prefs.
         */
        let mut results: HashMap<String, HashSet<String>> = HashMap::new();

        for (feature_name, props) in state.gecko_prefs_with_state.iter_mut() {
            let mut has_matching_recipes = false;
            for RecipeData {
                experiment:
                    Experiment {
                        slug,
                        feature_ids,
                        branches,
                        ..
                    },
                experiment_enrollment,
                branch_slug,
            } in &recipe_data
            {
                if feature_ids.contains(feature_name)
                    && matches!(
                        experiment_enrollment.status,
                        EnrollmentStatus::Enrolled { .. }
                    )
                {
                    let branch = match branches.iter().find(|branch| &branch.slug == branch_slug) {
                        Some(b) => b,
                        None => continue,
                    };
                    has_matching_recipes = true;
                    for (feature, prop_name, prop_value) in branch.get_feature_props_and_values() {
                        if feature == *feature_name && props.contains_key(&prop_name) {
                            // set the enrollment_value for this gecko pref.
                            // rollouts and experiments on the same feature will
                            // both set the value here, but rollouts will happen
                            // first, and will therefore be overridden by
                            // experiments.
                            props.entry(prop_name.clone()).and_modify(|pref_state| {
                                pref_state.enrollment_value = Some(PrefEnrollmentData {
                                    pref_value: prop_value.clone(),
                                    feature_id: feature,
                                    variable: prop_name,
                                });
                                results
                                    .entry(pref_state.gecko_pref.pref.clone())
                                    .or_default()
                                    .insert(slug.clone());
                            });
                        }
                    }
                }
            }

            if !has_matching_recipes {
                for (_, pref_state) in props.iter_mut() {
                    pref_state.enrollment_value = None;
                }
            }
        }

        // obtain a list of all Gecko pref states for which there is an enrollment value
        let mut set_state_list = Vec::new();
        state.gecko_prefs_with_state.iter().for_each(|(_, props)| {
            props.iter().for_each(|(_, pref_state)| {
                if pref_state.enrollment_value.is_some() {
                    set_state_list.push(pref_state.clone());
                }
            });
        });
        // tell the handler to set the aforementioned Gecko prefs
        self.handler.set_gecko_prefs_state(set_state_list);

        results
    }
}

pub fn query_gecko_pref_store(
    gecko_pref_store: Option<Arc<GeckoPrefStore>>,
    args: &[Value],
) -> Result<Value> {
    if args.len() != 1 {
        return Err(NimbusError::TransformParameterError(
            "gecko_pref transform preferenceIsUserSet requires exactly 1 parameter".into(),
        ));
    }

    let gecko_pref = match serde_json::from_value::<String>(args.first().unwrap().clone()) {
        Ok(v) => v,
        Err(e) => return Err(NimbusError::JSONError("gecko_pref = nimbus::stateful::gecko_prefs::query_gecko_prefs_store::serde_json::from_value".into(), e.to_string()))
    };

    Ok(gecko_pref_store
        .map(|store| Value::Bool(store.pref_is_user_set(&gecko_pref)))
        .unwrap_or(Value::Bool(false)))
}
