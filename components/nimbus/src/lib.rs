// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod defaults;
mod enrollment;
mod evaluator;
mod matcher;
mod sampling;
mod strings;
mod targeting;

pub mod error;
pub mod schema;
pub mod versioning;

pub use enrollment::{EnrolledFeature, EnrollmentStatus};
pub use error::{NimbusError, Result};
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;
pub use matcher::AppContext;
pub use schema::*;
pub use targeting::NimbusTargetingHelper;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        mod behavior;
        mod client;
        mod config;
        mod dbcache;
        mod updating;

        pub mod nimbus_client;
        pub mod persistence;

        pub use crate::nimbus_client::*;
        pub use config::RemoteSettingsConfig;
    } else {
        pub mod stateless {
            pub mod cirrus_client;
        }

        pub use crate::stateless::cirrus_client::*;
    }
}

// Exposed for Example only
pub use evaluator::TargetingAttributes;

#[cfg(test)]
mod tests;

cfg_if::cfg_if! {
    if #[cfg(any(
        feature = "stateful-uniffi-bindings",
        feature = "stateless-uniffi-bindings"
    ))] {
        use enrollment::{EnrollmentChangeEvent, EnrollmentChangeEventType};
        use serde_json::Value;

        impl UniffiCustomTypeConverter for JsonObject {
            type Builtin = String;

            fn into_custom(val: Self::Builtin) -> uniffi::Result<Self> {
                let json: Value = serde_json::from_str(&val)?;

                match json.as_object() {
                    Some(obj) => Ok(obj.clone()),
                    _ => Err(uniffi::deps::anyhow::anyhow!(
                        "Unexpected JSON-non-object in the bagging area"
                    )),
                }
            }

            fn from_custom(obj: Self) -> Self::Builtin {
                serde_json::Value::Object(obj).to_string()
            }
        }

        include!(concat!(env!("OUT_DIR"), "/nimbus.uniffi.rs"));
    }
}
