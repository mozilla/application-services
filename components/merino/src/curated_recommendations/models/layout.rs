/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

/// A named layout configuration containing one or more responsive layout breakpoints.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Layout {
    /// Name identifier for this layout (e.g. `"4-large"`, `"3-medium"`).
    pub name: String,
    /// Responsive layout variants for different screen widths.
    #[serde(rename = "responsiveLayouts")]
    pub responsive_layouts: Vec<ResponsiveLayout>,
}

/// A layout variant for a specific column count, defining how tiles are arranged.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct ResponsiveLayout {
    /// Number of columns in this layout variant.
    #[serde(rename = "columnCount")]
    pub column_count: i32,
    /// Tile configurations for this layout.
    pub tiles: Vec<Tile>,
}

/// Properties for a single tile within a responsive layout.
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Tile {
    /// Display size of the tile (e.g. `"large"`, `"medium"`, `"small"`).
    pub size: String,
    /// Zero-based position index of this tile within the layout.
    pub position: i32,
    /// Whether this tile position may contain an advertisement.
    #[serde(rename = "hasAd")]
    pub has_ad: bool,
    /// Whether this tile should display an article excerpt.
    #[serde(rename = "hasExcerpt")]
    pub has_excerpt: bool,
}
