/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::BridgeType;

#[derive(Clone, Debug)]
pub struct PushConfiguration {
    /// host name:port
    pub server_host: String,

    /// http protocol (for mobile, bridged connections "https")
    pub http_protocol: String,

    /// bridge protocol ("fcm")
    pub bridge_type: BridgeType,

    /// Service enabled flag
    pub enabled: bool,

    /// How often to ping server (1800s)
    pub ping_interval: u64,

    /// Sender/Application ID value
    pub sender_id: String,

    /// OS Path to the database
    pub database_path: String,
}

impl Default for PushConfiguration {
    fn default() -> PushConfiguration {
        PushConfiguration {
            server_host: String::from("push.services.mozilla.com"),
            http_protocol: String::from("https"),
            bridge_type: Default::default(),
            enabled: true,
            ping_interval: 1800,
            sender_id: String::from(""),
            database_path: String::from(""),
        }
    }
}
