// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
#![allow(clippy::too_many_arguments)]

use crate::{
    enrollment::{
        map_features_by_feature_id, EnrolledFeatureConfig, EnrollmentChangeEvent,
        EnrollmentsEvolver, ExperimentEnrollment,
    },
    error::CirrusClientError,
    AvailableRandomizationUnits, Experiment, NimbusError, Result, TargetingAttributes,
};
use serde_derive::*;
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
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

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentRequest {
    pub client_id: Option<String>,
    pub context: TargetingAttributes,
    pub is_user_participating: Option<bool>,
    pub prev_experiments: Option<Vec<EnrolledFeatureConfig>>,
    pub next_experiments: Vec<Experiment>,
    pub prev_enrollments: Option<Vec<ExperimentEnrollment>>,
}

#[derive(Default)]
pub struct CirrusClient {}

impl CirrusClient {
    pub fn new() -> Self {
        Self {}
    }

    pub fn handle_enrollment(&self, request: EnrollmentRequest) -> Result<EnrollmentResponse> {
        let EnrollmentRequest {
            client_id,
            context,
            is_user_participating,
            prev_experiments,
            next_experiments,
            prev_enrollments,
        } = request;
        let client_id = if let Some(client_id) = client_id {
            client_id
        } else {
            return Err(NimbusError::CirrusError(
                CirrusClientError::RequestMissingParameter("client_id".to_string()),
            ));
        };

        self.enroll(
            client_id,
            context,
            is_user_participating.unwrap_or(true),
            &match prev_experiments {
                Some(exps) => exps,
                None => Vec::new(),
            },
            &next_experiments,
            &match prev_enrollments {
                Some(enrollments) => enrollments,
                None => Vec::new(),
            },
        )
    }

    pub(crate) fn enroll(
        &self,
        client_id: String,
        context: TargetingAttributes,
        is_user_participating: bool,
        prev_experiments: &[EnrolledFeatureConfig],
        next_experiments: &[Experiment],
        prev_enrollments: &[ExperimentEnrollment],
    ) -> Result<EnrollmentResponse> {
        let nimbus_id = Uuid::new_v4();
        let available_randomization_units =
            AvailableRandomizationUnits::with_client_id(client_id.as_str());
        let th = context.into();
        let enrollments_evolver =
            EnrollmentsEvolver::new(&nimbus_id, &available_randomization_units, &th);

        let (enrollments, events) = enrollments_evolver.evolve_enrollments(
            is_user_participating,
            prev_experiments,
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
