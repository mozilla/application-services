/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines all the information needed to match a user with an experiment
//! this module should also include a `match` function of some sort that does the matching
//! it has two main types, the `matcher` retrieved from the server, and the `AppContext`
//! from the client
//! Note: This could be where the logic to evaluate the filter_expressions lies
use serde_derive::*;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Matcher {
    pub app_id: Option<String>,
    pub app_display_version: Option<String>,
    pub app_min_version: Option<String>,
    pub app_max_version: Option<String>,
    pub locale: Option<String>,
    pub device_manufacturer: Option<String>,
    pub device_model: Option<String>,
    pub regions: Vec<String>,
    pub debug_tags: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct AppContext {
    pub app_id: Option<String>,
    pub app_version: Option<String>,
    pub locale: Option<String>,
    pub device_manufacturer: Option<String>,
    pub device_model: Option<String>,
    pub region: Option<String>,
    pub debug_tag: Option<String>,
}
