/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_derive::*;
use std::collections::HashMap;

// Known record formats.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalEngine {
    pub version: usize,
    #[serde(rename = "syncID")]
    pub sync_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalRecord {
    #[serde(rename = "syncID")]
    pub sync_id: String,
    #[serde(rename = "storageVersion")]
    pub storage_version: usize,
    pub engines: HashMap<String, MetaGlobalEngine>,
    pub declined: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct CryptoKeysRecord {
    pub id: String,
    pub collection: String,
    pub default: [String; 2],
    pub collections: HashMap<String, [String; 2]>,
}
