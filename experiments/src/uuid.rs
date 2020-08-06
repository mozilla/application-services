/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module is reponsible for generating a random uuid per device
//! the uuid is persisted for later retrieval
//! It is also possible to manually set the uuid by passing in a custome
//! uuid as a part of the Config parameter in `Experiments`
use crate::Config;
pub use uuid::Uuid;

/// Generate uuid for `Experiments`
/// # Arguments
/// - `config`: Optional configurations that may include a custom uuid to be set.
pub fn generate_uuid(config: Option<Config>) -> uuid::Uuid {
    if let Some(config) = config {
        if let Some(uuid) = config.uuid {
            // TODO: Maybe return an error here if the custom uuid is invalid?
            return uuid::Uuid::parse_str(&uuid).unwrap_or_else(|_| uuid::Uuid::new_v4());
        }
    }
    uuid::Uuid::new_v4()
}
