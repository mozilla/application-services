/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::layout::Layout;
use super::response::RecommendationDataItem;

/// Container for all categorized recommendation feeds returned by the API.
///
/// Each field corresponds to a content category or special feed type. Fields are
/// `None` when the category was not requested or has no content available.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Feeds {
    /// High-priority "need to know" recommendations.
    #[uniffi(default)]
    pub need_to_know: Option<CuratedRecommendationsBucket>,
    /// Fakespot product review recommendations.
    #[uniffi(default)]
    pub fakespot: Option<FakespotFeed>,
    /// Top stories section.
    #[uniffi(default)]
    pub top_stories_section: Option<FeedSection>,
    #[uniffi(default)]
    pub business: Option<FeedSection>,
    #[uniffi(default)]
    pub career: Option<FeedSection>,
    #[uniffi(default)]
    pub arts: Option<FeedSection>,
    #[uniffi(default)]
    pub food: Option<FeedSection>,
    #[uniffi(default)]
    pub health: Option<FeedSection>,
    #[uniffi(default)]
    pub home: Option<FeedSection>,
    #[uniffi(default)]
    pub finance: Option<FeedSection>,
    #[uniffi(default)]
    pub government: Option<FeedSection>,
    #[uniffi(default)]
    pub sports: Option<FeedSection>,
    #[uniffi(default)]
    pub tech: Option<FeedSection>,
    #[uniffi(default)]
    pub travel: Option<FeedSection>,
    #[uniffi(default)]
    pub education: Option<FeedSection>,
    #[uniffi(default)]
    pub hobbies: Option<FeedSection>,
    #[serde(rename = "society-parenting")]
    #[uniffi(default)]
    pub society_parenting: Option<FeedSection>,
    #[serde(rename = "education-science")]
    #[uniffi(default)]
    pub education_science: Option<FeedSection>,
    #[uniffi(default)]
    pub society: Option<FeedSection>,
}

/// A ranked list of curated recommendations with an optional title.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsBucket {
    /// The recommendations in this bucket.
    pub recommendations: Vec<RecommendationDataItem>,
    /// Optional display title for this bucket.
    #[uniffi(default)]
    pub title: Option<String>,
}

/// A categorized feed section containing recommendations and responsive layout configuration.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FeedSection {
    /// The display position of this section within the overall feed.
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    /// The recommendations in this section.
    pub recommendations: Vec<RecommendationDataItem>,
    /// Display title for this section.
    pub title: String,
    /// Optional subtitle for this section.
    #[uniffi(default)]
    pub subtitle: Option<String>,
    /// Responsive layout configuration for rendering this section.
    pub layout: Layout,
    /// Whether the user is following this section.
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    /// Whether the user has blocked this section.
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

/// A feed of Fakespot product review recommendations.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotFeed {
    /// The recommended products.
    pub products: Vec<FakespotProduct>,
    /// Default category name for display.
    #[serde(rename = "defaultCategoryName")]
    pub default_category_name: String,
    /// Header copy text displayed above the product list.
    #[serde(rename = "headerCopy")]
    pub header_copy: String,
    /// Footer copy text displayed below the product list.
    #[serde(rename = "footerCopy")]
    pub footer_copy: String,
    /// Call-to-action link for the Fakespot feed.
    pub cta: FakespotCta,
}

/// Details for a single Fakespot product recommendation.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotProduct {
    /// Unique product identifier.
    id: String,
    /// Product title.
    title: String,
    /// Product category.
    category: String,
    /// URL of the product image.
    #[serde(rename = "imageUrl")]
    image_url: String,
    /// URL of the product page.
    url: String,
}

/// Call-to-action link for the Fakespot feed.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotCta {
    /// Display text for the call-to-action button.
    #[serde(rename = "ctaCopy")]
    pub cta_copy: String,
    /// URL the call-to-action links to.
    pub url: String,
}
