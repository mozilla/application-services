// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::matcher::RequestContext;
use crate::{
    enrollment::{
        map_features_by_feature_id, EnrolledFeatureConfig, EnrollmentChangeEvent,
        EnrollmentsEvolver, ExperimentEnrollment,
    },
    error::CirrusClientError,
    AppContext, AvailableRandomizationUnits, Experiment, NimbusError, NimbusTargetingHelper,
    Result, TargetingAttributes,
};
use serde_derive::*;
use serde_json::{from_str, to_string};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

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
    pub app_context: AppContext,
    pub request_context: RequestContext,
    #[serde(default = "default_true")]
    pub is_user_participating: bool,
    pub next_experiments: Vec<Experiment>,
    #[serde(default)]
    pub prev_enrollments: Vec<ExperimentEnrollment>,
}

impl Default for EnrollmentRequest {
    fn default() -> Self {
        Self {
            client_id: None,
            app_context: Default::default(),
            request_context: Default::default(),
            is_user_participating: true,
            next_experiments: Default::default(),
            prev_enrollments: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct CirrusClient {}

impl CirrusClient {
    pub fn new() -> Self {
        Self {}
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
            app_context,
            request_context,
            is_user_participating,
            next_experiments,
            prev_enrollments,
        } = from_str(request.as_str())?;
        let client_id = if let Some(client_id) = client_id {
            client_id
        } else {
            return Err(NimbusError::CirrusError(
                CirrusClientError::RequestMissingParameter("client_id".to_string()),
            ));
        };

        let context = TargetingAttributes::new(app_context, request_context);

        Ok(to_string(&self.enroll(
            client_id,
            context,
            is_user_participating,
            &next_experiments,
            &prev_enrollments,
        )?)?)
    }

    pub(crate) fn enroll(
        &self,
        client_id: String,
        targeting_attributes: TargetingAttributes,
        is_user_participating: bool,
        next_experiments: &[Experiment],
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<EnrollmentResponse> {
        // nimbus_id is set randomly here because all applications using the CirrusClient will not
        // be using nimbus_id as the bucket randomization unit. This will be refactored out as a
        // part of https://mozilla-hub.atlassian.net/browse/EXP-3401
        let nimbus_id = Uuid::new_v4();
        let available_randomization_units =
            AvailableRandomizationUnits::with_client_id(client_id.as_str());
        let th = NimbusTargetingHelper::new(targeting_attributes);
        let enrollments_evolver =
            EnrollmentsEvolver::new(&nimbus_id, &available_randomization_units, &th);

        let (enrollments, events) = enrollments_evolver
            .evolve_enrollments::<EnrolledFeatureConfig>(
                is_user_participating,
                Default::default(),
                next_experiments,
                prev_enrollments,
            )?;

        let enrolled_feature_config_map =
            map_features_by_feature_id(&enrollments, next_experiments);

        Ok(EnrollmentResponse {
            enrolled_feature_config_map,
            enrollments,
            events,
        })
    }
}

include!(concat!(env!("OUT_DIR"), "/cirrus.uniffi.rs"));
