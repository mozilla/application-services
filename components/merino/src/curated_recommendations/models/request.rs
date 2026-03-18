/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::locale::CuratedRecommendationLocale;

// Configuration options for initializing a `CuratedRecommendationsClient`
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsConfig {
    pub base_host: Option<String>,
    pub user_agent_header: String,
}

// Configuration settings for a Section
#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct SectionSettings {
    #[serde(rename = "sectionId")]
    pub section_id: String,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

// Information required to request curated recommendations
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsRequest {
    pub locale: CuratedRecommendationLocale,
    #[uniffi(default = None)]
    pub region: Option<String>,
    #[uniffi(default = Some(100))]
    pub count: Option<i32>,
    #[uniffi(default = None)]
    pub topics: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub feeds: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub sections: Option<Vec<SectionSettings>>,
    #[serde(rename = "experimentName")]
    #[uniffi(default = None)]
    pub experiment_name: Option<String>,
    #[serde(rename = "experimentBranch")]
    #[uniffi(default = None)]
    pub experiment_branch: Option<String>,
    #[serde(rename = "enableInterestPicker", default)]
    #[uniffi(default = false)]
    pub enable_interest_picker: bool,
}
