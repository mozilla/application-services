/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::feeds::Feeds;

// Response schema for a list of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsResponse {
    #[serde(rename = "recommendedAt")]
    pub recommended_at: i64,
    pub data: Vec<RecommendationDataItem>,
    #[uniffi(default = None)]
    pub feeds: Option<Feeds>,
    #[serde(rename = "interestPicker")]
    #[uniffi(default = None)]
    pub interest_picker: Option<InterestPicker>,
}

// Specifies the display order (receivedFeedRank) and a list of sections (referenced by sectionId) for interest bubbles.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct InterestPicker {
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    pub title: String,
    pub subtitle: String,
    pub sections: Vec<InterestPickerSection>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct InterestPickerSection {
    #[serde(rename = "sectionId")]
    pub section_id: String,
}

// Curated Recommendation Information
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct RecommendationDataItem {
    #[serde(rename = "corpusItemId")]
    pub corpus_item_id: String,
    #[serde(rename = "scheduledCorpusItemId")]
    pub scheduled_corpus_item_id: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    #[uniffi(default = None)]
    pub topic: Option<String>,
    pub publisher: String,
    #[serde(rename = "isTimeSensitive")]
    pub is_time_sensitive: bool,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "tileId")]
    pub tile_id: i64,
    #[serde(rename = "receivedRank")]
    pub received_rank: i64,
}

