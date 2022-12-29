/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod client;
mod error;

pub use crate::{
    client::{
        MerinoClient, MerinoClientFetchOptions, MerinoClientSettings, MerinoServer,
        MerinoSuggestion,
    },
    error::MerinoClientError,
};

uniffi_macros::include_scaffolding!("merino_client");
