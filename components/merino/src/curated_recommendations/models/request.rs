/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::locale::CuratedRecommendationLocale;

/// Configuration options for initializing a [`CuratedRecommendationsClient`](crate::curated_recommendations::CuratedRecommendationsClient).
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsConfig {
    /// Optional custom base host URL. Defaults to the production Merino service if `None`.
    pub base_host: Option<String>,
    /// The `User-Agent` header value to send with API requests.
    pub user_agent_header: String,
}

/// User preferences for a content section, controlling whether it is followed or blocked.
#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct SectionSettings {
    /// Unique identifier for the section.
    #[serde(rename = "sectionId")]
    pub section_id: String,
    /// Whether the user has opted to follow this section.
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    /// Whether the user has opted to block this section.
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

/// Parameters for requesting curated recommendations from the Merino API.
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsRequest {
    /// The locale to use when selecting recommendations.
    pub locale: CuratedRecommendationLocale,
    /// Optional ISO 3166-1 region code (e.g. `"US"`, `"GB"`) to further refine results.
    #[uniffi(default = None)]
    pub region: Option<String>,
    /// Maximum number of recommendations to return. Defaults to 100 if not specified.
    #[uniffi(default = Some(100))]
    pub count: Option<i32>,
    /// Optional list of topic slugs to filter recommendations by (e.g. `"business"`, `"tech"`).
    #[uniffi(default = None)]
    pub topics: Option<Vec<String>>,
    /// Optional list of feed types to include in the response (e.g. `"sections"`).
    #[uniffi(default = None)]
    pub feeds: Option<Vec<String>>,
    /// Optional per-section follow/block preferences.
    #[uniffi(default = None)]
    pub sections: Option<Vec<SectionSettings>>,
    /// Optional experiment name for server-side A/B testing.
    #[serde(rename = "experimentName")]
    #[uniffi(default = None)]
    pub experiment_name: Option<String>,
    /// Optional experiment branch for server-side A/B testing.
    #[serde(rename = "experimentBranch")]
    #[uniffi(default = None)]
    pub experiment_branch: Option<String>,
    /// Whether to include the interest picker in the response.
    #[serde(rename = "enableInterestPicker", default)]
    #[uniffi(default = false)]
    pub enable_interest_picker: bool,
}
