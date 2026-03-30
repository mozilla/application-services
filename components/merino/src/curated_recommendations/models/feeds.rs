/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::layout::Layout;
use super::response::RecommendationDataItem;

/// A categorized feed section containing recommendations and responsive layout configuration.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FeedSection {
    /// Identifier for this feed section (the key from the API response map,
    /// e.g. `"top_stories_section"`, `"travel"`, `"arts"`).
    #[serde(rename = "feedId", default)]
    pub feed_id: String,
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

/// Deserializes the `feeds` field from a JSON map of section name to section data
/// into an `Option<Vec<FeedSection>>`, populating each section's `feed_id` from its map key.
pub(crate) fn deserialize_feeds<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<FeedSection>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map: Option<HashMap<String, serde_json::Value>> = Option::deserialize(deserializer)?;
    match map {
        None => Ok(None),
        Some(map) => {
            let mut sections = Vec::with_capacity(map.len());
            for (key, mut value) in map {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("feedId".to_string(), serde_json::Value::String(key.clone()));
                }
                let section: FeedSection =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                sections.push(section);
            }
            sections.sort_by_key(|s| s.received_feed_rank);
            Ok(Some(sections))
        }
    }
}
