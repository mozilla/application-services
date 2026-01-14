/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use serde_derive::{Deserialize, Serialize};

use crate::EnrollmentStatus;
use crate::enrollment::ExperimentEnrollment;
#[cfg(feature = "stateful")]
use crate::enrollment::PreviousGeckoPrefState;
pub use crate::metrics::detail::*;

#[derive(Serialize, Deserialize, Clone)]
pub struct EnrollmentStatusExtraDef {
    pub branch: Option<String>,
    pub conflict_slug: Option<String>,
    pub error_string: Option<String>,
    pub reason: Option<String>,
    pub slug: Option<String>,
    pub status: Option<String>,
    #[cfg(not(feature = "stateful"))]
    pub user_id: Option<String>,
    #[cfg(feature = "stateful")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_gecko_pref_states: Option<Vec<PreviousGeckoPrefState>>,
}

impl From<ExperimentEnrollment> for EnrollmentStatusExtraDef {
    fn from(enrollment: ExperimentEnrollment) -> Self {
        let mut branch_value: Option<String> = None;
        let mut reason_value: Option<String> = None;
        let mut error_value: Option<String> = None;
        match &enrollment.status {
            EnrollmentStatus::Enrolled { reason, branch, .. } => {
                branch_value = Some(branch.to_owned());
                reason_value = Some(reason.to_string());
            }
            EnrollmentStatus::Disqualified { reason, branch, .. } => {
                branch_value = Some(branch.to_owned());
                reason_value = Some(reason.to_string());
            }
            EnrollmentStatus::NotEnrolled { reason } => {
                reason_value = Some(reason.to_string());
            }
            EnrollmentStatus::WasEnrolled { branch, .. } => branch_value = Some(branch.to_owned()),
            EnrollmentStatus::Error { reason } => {
                error_value = Some(reason.to_owned());
            }
        }
        EnrollmentStatusExtraDef {
            branch: branch_value,
            conflict_slug: None,
            error_string: error_value,
            reason: reason_value,
            slug: Some(enrollment.slug),
            status: Some(enrollment.status.name()),
            #[cfg(not(feature = "stateful"))]
            user_id: None,
            #[cfg(feature = "stateful")]
            prev_gecko_pref_states: None,
        }
    }
}

#[cfg(feature = "stateful")]
mod detail {
    use crate::EnrolledFeature;

    use super::EnrollmentStatusExtraDef;

    #[uniffi::trait_interface]
    pub trait MetricsHandler: Send + Sync {
        fn record_enrollment_statuses(
            &self,
            enrollment_status_extras: Vec<EnrollmentStatusExtraDef>,
        );

        fn record_feature_activation(&self, event: FeatureExposureExtraDef);

        fn record_feature_exposure(&self, event: FeatureExposureExtraDef);

        fn record_malformed_feature_config(&self, event: MalformedFeatureConfigExtraDef);

        fn submit_targeting_context(&self);
    }

    #[derive(Clone)]
    pub struct FeatureExposureExtraDef {
        pub branch: Option<String>,
        pub slug: String,
        pub feature_id: String,
    }

    impl From<EnrolledFeature> for FeatureExposureExtraDef {
        fn from(value: EnrolledFeature) -> Self {
            Self {
                feature_id: value.feature_id,
                branch: value.branch,
                slug: value.slug,
            }
        }
    }

    #[derive(Clone, Default)]
    #[cfg_attr(test, derive(Debug, Eq, PartialEq))]
    pub struct MalformedFeatureConfigExtraDef {
        pub slug: Option<String>,
        pub branch: Option<String>,
        pub feature_id: String,
        pub part: String,
    }

    impl MalformedFeatureConfigExtraDef {
        pub(crate) fn from_feature_and_part(value: EnrolledFeature, part: String) -> Self {
            Self {
                slug: Some(value.slug),
                branch: value.branch,
                feature_id: value.feature_id,
                part,
            }
        }

        pub(crate) fn new(feature_id: String, part: String) -> Self {
            Self {
                feature_id,
                part,
                ..Default::default()
            }
        }
    }
}

#[cfg(not(feature = "stateful"))]
mod detail {
    use super::EnrollmentStatusExtraDef;

    #[uniffi::trait_interface]
    pub trait MetricsHandler: Send + Sync {
        fn record_enrollment_statuses_v2(
            &self,
            enrollment_status_extras: Vec<EnrollmentStatusExtraDef>,
            nimbus_user_id: Option<String>,
        );
    }
}
