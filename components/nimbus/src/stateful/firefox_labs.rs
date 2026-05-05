// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::enrollment::EnrollmentChangeEvent;

pub const FIREFOX_LABS_FEEDBACK_URL_KEY: &str = "feedback";

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct FirefoxLabsMetadata {
    pub slug: String,
    pub enrolled: bool,
    pub title_string_id: String,
    pub description_string_id: String,
    pub feedback_url: Option<String>,
    pub requires_restart: bool,
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct FirefoxLabsEnrollResult {
    pub status: FirefoxLabsEnrollStatus,
    pub enrollment_change_events: Vec<EnrollmentChangeEvent>,
}

#[derive(Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum FirefoxLabsEnrollStatus {
    Enrolled,
    AlreadyEnrolled,
    NoExperiment,
    NotFirefoxLabsOptIn,
    FeatureConflict,
    Error,
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct FirefoxLabsUnenrollResult {
    pub status: FirefoxLabsUnenrollStatus,
    pub enrollment_change_events: Vec<EnrollmentChangeEvent>,
}

#[derive(Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum FirefoxLabsUnenrollStatus {
    Unenrolled,
    AlreadyUnenrolled,
    NoExperiment,
    NotFirefoxLabsOptIn,
    Error,
}
