/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use super::layout::Layout;
use super::response::RecommendationDataItem;

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
