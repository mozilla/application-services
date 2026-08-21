/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod backends;
pub mod command_line;
pub(crate) mod defaults;
mod editing;
pub mod error;
pub(crate) mod frontend;
pub mod intermediate_representation;
pub mod parser;
pub(crate) mod schema;
pub mod util;
pub use url::Url;

cfg_if::cfg_if! {
    if #[cfg(feature = "client-lib")] {
        pub mod client;
        pub use client::JsonObject;
        pub use crate::client::*;
    }
}

#[cfg(test)]
pub mod fixtures;

// Custom type definitions need to live in the root module while using UDL
// https://github.com/mozilla/uniffi-rs/issues/2968

#[cfg(all(feature = "uniffi-bindings", feature = "client-lib"))]
uniffi::custom_type!(JsonObject, String, {
    remote,
    try_lift: |val| {
        let json: serde_json::Value = serde_json::from_str(&val)?;

        match json.as_object() {
            Some(obj) => Ok(obj.to_owned()),
            _ => Err(uniffi::deps::anyhow::anyhow!(
                "Unexpected JSON-non-object in the bagging area"
            )),
        }
    },
    lower: |obj| serde_json::Value::Object(obj).to_string(),
});

#[cfg(all(feature = "uniffi-bindings", feature = "client-lib"))]
uniffi::custom_type!(Url, String, {
    remote,
    try_lift: |val| Ok(val.parse()?),
    lower: |obj| obj.as_str().to_string(),
});

const SUPPORT_URL_LOADING: bool = true;
