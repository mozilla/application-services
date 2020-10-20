/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the custom configurations that consumers can set.
//! Those configurations override default values and can be used to set a custom server url,
//! collection name, bucket name and uuid.
//! The purpose of the configuration parameters is to allow consumers an easy debugging option,
//! and the ability to be explicit about the server.

/// Optional custom configuration
/// Currently includes the following:
/// - `server_url`: The url for the settings server that would be used to retrieve experiments
/// - `bucket_name`: The name of the bucket containing the collection on the server
#[derive(Debug, Clone)]
pub struct RemoteSettingsConfig {
    pub server_url: String,
    pub collection_name: String,
    pub bucket_name: String,
}
