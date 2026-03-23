/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::feeds::FeedSection;

/// Top-level response from the Merino curated recommendations API.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsResponse {
    /// Timestamp (in milliseconds since epoch) when the recommendations were generated.
    #[serde(rename = "recommendedAt")]
    pub recommended_at: i64,
    /// The list of recommended items.
    pub data: Vec<RecommendationDataItem>,
    /// Optional categorized feeds (e.g. by topic section).
    #[uniffi(default = None)]
    pub feeds: Option<Vec<FeedSection>>,
    /// Optional interest picker configuration for displaying section selection UI.
    #[serde(rename = "interestPicker")]
    #[uniffi(default = None)]
    pub interest_picker: Option<InterestPicker>,
}

/// Configuration for the interest picker UI, which lets users select preferred content sections.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct InterestPicker {
    /// The display position of the interest picker within the feed.
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    /// Title text for the interest picker.
    pub title: String,
    /// Subtitle text for the interest picker.
    pub subtitle: String,
    /// The sections available for the user to choose from.
    pub sections: Vec<InterestPickerSection>,
}

/// A section entry within the interest picker.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct InterestPickerSection {
    /// Unique identifier for the section.
    #[serde(rename = "sectionId")]
    pub section_id: String,
}

/// A single curated recommendation item.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct RecommendationDataItem {
    /// Unique identifier for the corpus item.
    #[serde(rename = "corpusItemId")]
    pub corpus_item_id: String,
    /// Unique identifier for the scheduled corpus item.
    #[serde(rename = "scheduledCorpusItemId")]
    pub scheduled_corpus_item_id: String,
    /// URL of the recommended article.
    pub url: String,
    /// Title of the recommended article.
    pub title: String,
    /// Short excerpt or summary of the article.
    pub excerpt: String,
    /// Optional topic slug (e.g. `"business"`, `"government"`).
    #[uniffi(default = None)]
    pub topic: Option<String>,
    /// Name of the publisher.
    pub publisher: String,
    /// Whether the recommendation is time-sensitive (e.g. breaking news).
    #[serde(rename = "isTimeSensitive")]
    pub is_time_sensitive: bool,
    /// URL of the article's hero/thumbnail image.
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    /// Optional URL of the publisher's favicon.
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    /// Numeric tile identifier used for telemetry.
    #[serde(rename = "tileId")]
    pub tile_id: i64,
    /// The position rank at which this item was received from the server.
    #[serde(rename = "receivedRank")]
    pub received_rank: i64,
}
