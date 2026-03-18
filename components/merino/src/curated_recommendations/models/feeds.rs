/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::layout::Layout;
use super::response::RecommendationDataItem;

// Multiple lists of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Feeds {
    #[uniffi(default = None)]
    pub need_to_know: Option<CuratedRecommendationsBucket>,
    #[uniffi(default = None)]
    pub fakespot: Option<FakespotFeed>,
    #[uniffi(default = None)]
    pub top_stories_section: Option<FeedSection>,
    #[uniffi(default = None)]
    pub business: Option<FeedSection>,
    #[uniffi(default = None)]
    pub career: Option<FeedSection>,
    #[uniffi(default = None)]
    pub arts: Option<FeedSection>,
    #[uniffi(default = None)]
    pub food: Option<FeedSection>,
    #[uniffi(default = None)]
    pub health: Option<FeedSection>,
    #[uniffi(default = None)]
    pub home: Option<FeedSection>,
    #[uniffi(default = None)]
    pub finance: Option<FeedSection>,
    #[uniffi(default = None)]
    pub government: Option<FeedSection>,
    #[uniffi(default = None)]
    pub sports: Option<FeedSection>,
    #[uniffi(default = None)]
    pub tech: Option<FeedSection>,
    #[uniffi(default = None)]
    pub travel: Option<FeedSection>,
    #[uniffi(default = None)]
    pub education: Option<FeedSection>,
    #[uniffi(default = None)]
    pub hobbies: Option<FeedSection>,
    #[serde(rename = "society-parenting")]
    #[uniffi(default = None)]
    pub society_parenting: Option<FeedSection>,
    #[serde(rename = "education-science")]
    #[uniffi(default = None)]
    pub education_science: Option<FeedSection>,
    #[uniffi(default = None)]
    pub society: Option<FeedSection>,
}

// Ranked list of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsBucket {
    pub recommendations: Vec<RecommendationDataItem>,
    #[uniffi(default = None)]
    pub title: Option<String>,
}

// Ranked list of curated recommendations with responsive layout configs
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FeedSection {
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    pub recommendations: Vec<RecommendationDataItem>,
    pub title: String,
    #[uniffi(default = None)]
    pub subtitle: Option<String>,
    pub layout: Layout,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

// Fakespot product recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotFeed {
    pub products: Vec<FakespotProduct>,
    #[serde(rename = "defaultCategoryName")]
    pub default_category_name: String,
    #[serde(rename = "headerCopy")]
    pub header_copy: String,
    #[serde(rename = "footerCopy")]
    pub footer_copy: String,
    pub cta: FakespotCta,
}

// Fakespot product details
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotProduct {
    id: String,
    title: String,
    category: String,
    #[serde(rename = "imageUrl")]
    image_url: String,
    url: String,
}

// Fakespot CTA
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotCta {
    #[serde(rename = "ctaCopy")]
    pub cta_copy: String,
    pub url: String,
}
