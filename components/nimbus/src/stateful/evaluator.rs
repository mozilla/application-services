/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    enrollment::{EnrollmentStatus, ExperimentEnrollment},
    evaluator::split_locale,
    json::JsonObject,
    stateful::matcher::AppContext,
};
use chrono::{DateTime, Utc};
use serde_derive::*;
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TargetingAttributes {
    #[serde(flatten)]
    pub app_context: AppContext,
    pub language: Option<String>,
    pub region: Option<String>,
    #[serde(flatten)]
    pub recorded_context: Option<JsonObject>,
    pub is_already_enrolled: bool,
    pub days_since_install: Option<i32>,
    pub days_since_update: Option<i32>,
    pub active_experiments: HashSet<String>,
    pub enrollments: HashSet<String>,
    pub enrollments_map: HashMap<String, String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub current_date: DateTime<Utc>,
    pub nimbus_id: Option<String>,
}

impl From<AppContext> for TargetingAttributes {
    fn from(app_context: AppContext) -> Self {
        let (language, region) = app_context
            .locale
            .clone()
            .map(split_locale)
            .unwrap_or_else(|| (None, None));

        Self {
            app_context,
            language,
            region,
            ..Default::default()
        }
    }
}

impl TargetingAttributes {
    pub(crate) fn set_recorded_context(&mut self, recorded_context: JsonObject) {
        self.recorded_context = Some(recorded_context);
    }

    pub(crate) fn update_time_to_now(
        &mut self,
        now: DateTime<Utc>,
        install_date: &Option<DateTime<Utc>>,
        update_date: &Option<DateTime<Utc>>,
    ) {
        self.days_since_install = install_date.map(|then| (now - then).num_days() as i32);
        self.days_since_update = update_date.map(|then| (now - then).num_days() as i32);
        self.current_date = now;
    }

    pub(crate) fn update_enrollments(&mut self, enrollments: &[ExperimentEnrollment]) -> u32 {
        let mut modified_count = 0;
        for experiment_enrollment in enrollments {
            if self.update_enrollment(experiment_enrollment) {
                modified_count += 1;
            }
        }
        modified_count
    }

    pub(crate) fn update_enrollment(&mut self, enrollment: &ExperimentEnrollment) -> bool {
        match &enrollment.status {
            EnrollmentStatus::Enrolled { branch, .. } => {
                let inserted_active = self.active_experiments.insert(enrollment.slug.clone());
                let inserted_enrollment = self.enrollments.insert(enrollment.slug.clone());
                let updated_enrollment_map = self
                    .enrollments_map
                    .insert(enrollment.slug.clone(), branch.clone());

                inserted_active
                    || inserted_enrollment
                    || (updated_enrollment_map.is_some()
                        && &updated_enrollment_map.unwrap() != branch)
            }
            EnrollmentStatus::WasEnrolled { branch, .. }
            | EnrollmentStatus::Disqualified { branch, .. } => {
                let removed_active = self.active_experiments.remove(&enrollment.slug);
                let inserted_enrollment = self.enrollments.insert(enrollment.slug.clone());
                let updated_enrollments_map = self
                    .enrollments_map
                    .insert(enrollment.slug.clone(), branch.clone());

                removed_active
                    || inserted_enrollment
                    || (updated_enrollments_map.is_some()
                        && &updated_enrollments_map.unwrap() != branch)
            }
            EnrollmentStatus::NotEnrolled { .. } | EnrollmentStatus::Error { .. } => {
                let removed_active = self.active_experiments.remove(&enrollment.slug);
                let removed_enrollment = self.enrollments.remove(&enrollment.slug);
                let removed_from_enrollments_map = self.enrollments_map.remove(&enrollment.slug);

                removed_active || removed_enrollment || removed_from_enrollments_map.is_some()
            }
        }
    }
}
