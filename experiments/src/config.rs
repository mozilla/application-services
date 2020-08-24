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
/// - `uuid`: A custom user uuid that would otherwise be generated or loaded from persisted storage
/// - `collection_name`: The name of the collection on the server
/// - `bucket_name`: The name of the bucket containing the collection on the server
pub struct Config {
    pub server_url: Option<String>,
    pub uuid: Option<String>,
    pub collection_name: Option<String>,
    pub bucket_name: Option<String>,
}
