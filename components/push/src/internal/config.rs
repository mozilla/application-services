/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Provides configuration for the [PushManager](`crate::PushManager`)
//!

use std::{fmt::Display, str::FromStr};

use crate::PushError;
/// The types of supported native bridges.
///
/// FCM = Google Android Firebase Cloud Messaging
/// ADM = Amazon Device Messaging for FireTV
/// APNS = Apple Push Notification System for iOS
///
/// Please contact services back-end for any additional bridge protocols.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BridgeType {
    Fcm,
    Adm,
    Apns,
}

#[cfg(test)]
// To avoid a future footgun, the default implementation is only for tests
impl Default for BridgeType {
    fn default() -> Self {
        Self::Fcm
    }
}

impl Display for BridgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                BridgeType::Adm => "adm",
                BridgeType::Apns => "apns",
                BridgeType::Fcm => "fcm",
            }
        )
    }
}
#[derive(Clone, Debug)]
pub struct PushConfiguration {
    /// host name:port
    pub server_host: String,

    /// http protocol (for mobile, bridged connections "https")
    pub http_protocol: Protocol,

    /// bridge protocol ("fcm")
    pub bridge_type: BridgeType,

    /// Sender/Application ID value
    pub sender_id: String,

    /// OS Path to the database
    pub database_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Protocol {
    Https,
    Http,
}

impl Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Protocol::Http => "http",
                Protocol::Https => "https",
            }
        )
    }
}

impl Default for Protocol {
    fn default() -> Self {
        Protocol::Https
    }
}

impl FromStr for Protocol {
    type Err = PushError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "http" => Protocol::Http,
            "https" => Protocol::Https,
            _ => return Err(PushError::GeneralError("Invalid protocol".to_string())),
        })
    }
}

#[cfg(test)]
impl Default for PushConfiguration {
    fn default() -> PushConfiguration {
        PushConfiguration {
            server_host: String::from("push.services.mozilla.com"),
            http_protocol: Protocol::Https,
            bridge_type: Default::default(),
            sender_id: String::from(""),
            database_path: String::from(""),
        }
    }
}
