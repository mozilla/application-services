/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    enrollment::{
        map_features_by_feature_id, EnrolledFeatureConfig, EnrollmentChangeEvent,
        EnrollmentsEvolver, ExperimentEnrollment,
    },
    error::CirrusClientError,
    metrics::{EnrollmentStatusExtraDef, MetricsHandler},
    parse_experiments, AppContext, AvailableRandomizationUnits, Experiment, NimbusError,
    NimbusTargetingHelper, Result, TargetingAttributes,
};
use serde_derive::*;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

/// EnrollmentResponse is a DTO for the response from handling enrollment for a given client.
///
/// Definitions for the fields are as follows:
/// - `enrolled_feature_config_map`: This field contains the Map representation of the feature value JSON that should be merged with the default feature values.
/// - `enrollments`: This is the list of ExperimentEnrollments — it should be returned to the client.
/// - `events`: This is the list of EnrollmentChangeEvents. These events should be recorded to Glean.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentResponse {
    pub enrolled_feature_config_map: HashMap<String, EnrolledFeatureConfig>,
    pub enrollments: Vec<ExperimentEnrollment>,
    pub events: Vec<EnrollmentChangeEvent>,
}

impl fmt::Display for EnrollmentResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

fn default_true() -> bool {
    true
}

/// EnrollmentRequest is a DTO for the request for handling enrollment for a given client.
///
/// Definitions for the fields are as follows:
/// - `client_id`: This field is the client's id as defined by the calling application. Though this is an Option type, if it is missing the method will throw a NimbusError.
/// - `context`: The application context for the request. This value will be converted into TargetingAttributes.
/// - `is_user_participating`: Whether or not the user is participating in experimentation. Defaults to `true`
/// - `next_experiments`: The list of experiments for which enrollment should be evaluated.
/// - `prev_enrollments`: The client's current list of enrollments.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentRequest {
    pub client_id: Option<String>,
    pub request_context: Map<String, Value>,
    #[serde(default = "default_true")]
    pub is_user_participating: bool,
    #[serde(default)]
    pub prev_enrollments: Vec<ExperimentEnrollment>,
}

impl Default for EnrollmentRequest {
    fn default() -> Self {
        Self {
            client_id: None,
            request_context: Default::default(),
            is_user_participating: true,
            prev_enrollments: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct CirrusMutableState {
    experiments: Vec<Experiment>,
}

pub struct CirrusClient {
    app_context: AppContext,
    coenrolling_feature_ids: Vec<String>,
    state: Mutex<CirrusMutableState>,
    metrics_handler: Arc<Box<dyn MetricsHandler>>,
}

impl CirrusClient {
    pub fn new(
        app_context: String,
        metrics_handler: Box<dyn MetricsHandler>,
        coenrolling_feature_ids: Vec<String>,
    ) -> Result<Self> {
        let app_context: AppContext = match serde_json::from_str(&app_context) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("app_context = nimbus::stateless::cirrus_client::CirrusClient::new::serde_json::from_str".into(), e.to_string()))
        };
        Ok(Self {
            app_context,
            coenrolling_feature_ids,
            state: Default::default(),
            metrics_handler: Arc::new(metrics_handler),
        })
    }

    /// Handles an EnrollmentRequest, returning an EnrollmentResponse on success.
    ///
    /// This method is a helper method for the `enroll` method, which creates and calls into an
    /// EnrollmentsEvolver using only values found on the CirrusClient and values passed into this
    /// method. The information returned from this method can be used to merge the default feature
    /// values with the values applied by the enrolled experiments and to send enrollment-
    /// related Glean events.
    pub fn handle_enrollment(&self, request: String) -> Result<String> {
        let EnrollmentRequest {
            client_id,
            request_context,
            is_user_participating,
            prev_enrollments,
        } = match serde_json::from_str(request.as_str()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("EnrollmentRequest { .. } = nimbus::stateless::cirrus_client::CirrusClient::handle_enrollment::serde_json::from_str".into(), e.to_string()))
        };
        let client_id = if let Some(client_id) = client_id {
            client_id
        } else {
            return Err(NimbusError::CirrusError(
                CirrusClientError::RequestMissingParameter("client_id".to_string()),
            ));
        };

        Ok(match serde_json::to_string(&self.enroll(
            client_id,
            request_context,
            is_user_participating,
            &prev_enrollments,
        )?) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("return nimbus::stateless::cirrus_client::CirrusClient::handle_enrollment::serde_json::to_string".into(), e.to_string()))
        })
    }

    pub(crate) fn enroll(
        &self,
        user_id: String,
        request_context: Map<String, Value>,
        is_user_participating: bool,
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<EnrollmentResponse> {
        let available_randomization_units =
            AvailableRandomizationUnits::with_user_id(user_id.as_str());
        let targeting_attributes =
            TargetingAttributes::new(self.app_context.clone(), request_context);
        let mut targeting_helper = NimbusTargetingHelper::new(targeting_attributes);
        let coenrolling_ids = self
            .coenrolling_feature_ids
            .iter()
            .map(|s| s.as_str())
            .collect();
        let mut enrollments_evolver = EnrollmentsEvolver::new(
            &available_randomization_units,
            &mut targeting_helper,
            &coenrolling_ids,
        );
        let state = self.state.lock().unwrap();

        let (enrollments, events) = enrollments_evolver
            .evolve_enrollments::<EnrolledFeatureConfig>(
                is_user_participating,
                Default::default(),
                &state.experiments,
                prev_enrollments,
            )?;

        self.metrics_handler.record_enrollment_statuses(
            enrollments
                .iter()
                .cloned()
                .map(|e| {
                    let mut extra: EnrollmentStatusExtraDef = e.into();
                    extra.user_id = Some(user_id.clone());
                    extra
                })
                .collect(),
        );

        let enrolled_feature_config_map =
            map_features_by_feature_id(&enrollments, &state.experiments, &coenrolling_ids);

        Ok(EnrollmentResponse {
            enrolled_feature_config_map,
            enrollments,
            events,
        })
    }

    /// Sets the `experiments` value in the internal mutable state.
    ///
    /// This method does the following:
    /// 1) accepts and parses a JSON string into a list of Experiments
    /// 2) filters the list of experiments down to only experiments matching the client's `app_name`
    ///    and `channel`
    /// 3) writes the resulting value to the client's internal mutable state
    pub fn set_experiments(&self, experiments: String) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut exps: Vec<_> = Default::default();
        for exp in parse_experiments(&experiments)? {
            if exp.app_name.as_deref() == Some(&self.app_context.app_name)
                && exp.channel.as_deref() == Some(&self.app_context.channel)
            {
                exps.push(exp);
            }
        }
        state.experiments = exps;
        Ok(())
    }

    /// Retrieves `experiments` value from the internal mutable state.
    ///
    /// Currently only used in tests.
    pub fn get_experiments(&self) -> Result<Vec<Experiment>> {
        let state = self.state.lock().unwrap();
        Ok(state.experiments.clone())
    }
}

uniffi::include_scaffolding!("cirrus");
