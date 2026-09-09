// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod defaults;
mod enrollment;
mod evaluator;
mod json;
mod sampling;
mod strings;
mod targeting;

pub mod error;
pub mod metrics;
pub mod schema;

pub use crate::enrollment::{EnrolledFeature, EnrollmentStatus};
pub use crate::error::{NimbusError, Result};
#[cfg(debug_assertions)]
pub use crate::evaluator::evaluate_enrollment;
pub use crate::schema::*;
pub use crate::targeting::NimbusTargetingHelper;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {

        pub mod stateful;

        pub use remote_settings::{RemoteSettingsConfig, RemoteSettingsServer};

        pub use crate::stateful::nimbus_client::*;
        pub use crate::stateful::matcher::AppContext;
    } else {
        pub mod stateless;

        pub use crate::stateless::cirrus_client::*;
        pub use crate::stateless::matcher::AppContext;
    }
}

#[cfg(feature = "stateful-uniffi-bindings")]
use json::{JsonObject, PrefValue};

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::custom_type!(JsonObject, String, {
    remote,
    try_lift: |val| {
        let json: serde_json::Value = serde_json::from_str(&val)?;

        match json.as_object() {
            Some(obj) => Ok(obj.clone()),
            _ => Err(uniffi::deps::anyhow::anyhow!(
                "Unexpected JSON-non-object in the bagging area"
            )),
        }
    },
    lower: |obj| serde_json::Value::Object(obj).to_string(),
});

#[cfg(feature = "stateful-uniffi-bindings")]
uniffi::custom_type!(PrefValue, String, {
    remote,
    try_lift: |val| {
        // Raw strings that are not valid JSON (e.g. pref values read directly from Gecko)
        // should be treated as JSON string values.
        let json: serde_json::Value = match serde_json::from_str(&val) {
            Ok(json) => json,
            Err(_) => serde_json::Value::String(val),
        };
        let is_valid_pref_type = json.is_string() || json.is_boolean()
            || (json.is_number() && !json.is_f64()) || json.is_null();
        if is_valid_pref_type {
            Ok(json)
        } else {
            Err(anyhow::anyhow!(format!("Value {} is not a string, boolean, number, or null, or is a float", json)))
        }
    },
    lower: |val| {
        val.to_string()
    }
});

// Exposed for Example only
pub use evaluator::TargetingAttributes;

pub(crate) const SLUG_REPLACEMENT_PATTERN: &str = "{experiment}";

#[cfg(test)]
mod tests;
