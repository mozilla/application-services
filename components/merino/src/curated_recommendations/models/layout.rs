/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

// Representation of a responsive layout configuration with multiple column layouts
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Layout {
    pub name: String,
    #[serde(rename = "responsiveLayouts")]
    pub responsive_layouts: Vec<ResponsiveLayout>,
}

// Layout configurations within a column
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct ResponsiveLayout {
    #[serde(rename = "columnCount")]
    pub column_count: i32,
    pub tiles: Vec<Tile>,
}

// Properties for a single tile in a responsive layout
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Tile {
    pub size: String,
    pub position: i32,
    #[serde(rename = "hasAd")]
    pub has_ad: bool,
    #[serde(rename = "hasExcerpt")]
    pub has_excerpt: bool,
}
