/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

// Import the types uniffi plays with.
use crate::{
    device::{
        Capability as DeviceCapability, Location as DeviceLocation, PushSubscription,
        Type as DeviceType,
    },
    http_client::DeviceResponseCommon,
    migrator::{FxAMigrationResult, MigrationState},
    oauth::{AuthorizationPKCEParams, AuthorizationParameters},
    AccessTokenInfo, Device, FirefoxAccount, IntrospectInfo, Profile, ScopedKey,
};

// The type puts errors into 3 buckets, we could probably
// do without.
impl From<crate::Error> for FxAClientError {
    fn from(err: crate::Error) -> FxAClientError {
        match err.kind() {
            crate::ErrorKind::RemoteError { code: 401, .. }
            | crate::ErrorKind::NoRefreshToken
            | crate::ErrorKind::NoScopedKey(_)
            | crate::ErrorKind::NoCachedToken(_) => {
                log::warn!("Authentication error: {:?}", err);
                FxAClientError::Authentication
            }
            crate::ErrorKind::RequestError(_) => {
                log::warn!("Network error: {:?}", err);
                FxAClientError::Network
            }
            _ => {
                log::warn!("Unexpected error: {:?}", err);
                FxAClientError::Other
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum FxAClientError {
    #[error("Network error.")]
    Network,
    #[error("Authentication error.")]
    Authentication,
    #[error("Other error.")]
    Other,
}

include!(concat!(env!("OUT_DIR"), "/fxa_client.uniffi.rs"));
