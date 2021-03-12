/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines all the information needed to match a user with an experiment.
//! Soon it will also include a `match` function of some sort that does the matching.
//!
//! It has two main types, the `Matcher` retrieved from the server, and the `AppContext`
//! provided by the consuming client.
//!
use serde_derive::*;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Matcher {
    pub app_id: String,
    pub app_display_version: Option<String>,
    pub app_min_version: Option<String>,
    pub app_max_version: Option<String>,
    pub app_build: Option<String>,
    pub app_min_build: Option<String>,
    pub app_max_build: Option<String>,
    pub architecture: Option<String>,
    pub device_manufacturer: Option<String>,
    pub device_model: Option<String>,
    pub locale: Option<String>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub android_sdk_version: Option<String>,
    pub debug_tags: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct AppContext {
    pub app_id: String,
    pub app_version: Option<String>,
    pub app_build: Option<String>,
    pub architecture: Option<String>,
    pub device_manufacturer: Option<String>,
    pub device_model: Option<String>,
    pub locale: Option<String>,
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub android_sdk_version: Option<String>,
    pub debug_tag: Option<String>,
}
