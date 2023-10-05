use crate::{enrollment::ExperimentEnrollment, EnrollmentStatus};
use serde_derive::{Deserialize, Serialize};

pub trait MetricsHandler: Send + Sync {
    fn record_enrollment_statuses(&self, enrollment_status_extras: Vec<EnrollmentStatusExtraDef>);
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EnrollmentStatusExtraDef {
    pub branch: Option<String>,
    pub conflict_slug: Option<String>,
    pub error_string: Option<String>,
    pub reason: Option<String>,
    pub slug: Option<String>,
    pub status: Option<String>,
}

#[cfg(test)]
impl EnrollmentStatusExtraDef {
    pub fn branch(&self) -> &str {
        self.branch.as_ref().unwrap()
    }

    pub fn conflict_slug(&self) -> &str {
        self.conflict_slug.as_ref().unwrap()
    }

    pub fn error_string(&self) -> &str {
        self.error_string.as_ref().unwrap()
    }

    pub fn reason(&self) -> &str {
        self.reason.as_ref().unwrap()
    }

    pub fn slug(&self) -> &str {
        self.slug.as_ref().unwrap()
    }

    pub fn status(&self) -> &str {
        self.status.as_ref().unwrap()
    }
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
        }
    }
}
