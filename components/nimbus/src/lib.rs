// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(feature = "stateful")]
mod behavior;
#[cfg(feature = "stateful")]
mod client;
mod config;
#[cfg(feature = "stateful")]
mod dbcache;
mod defaults;
mod enrollment;
mod evaluator;
mod matcher;
mod sampling;
mod strings;
#[cfg(feature = "stateful")]
mod updating;

pub mod error;
#[cfg(feature = "stateful")]
pub mod nimbus_client;
#[cfg(feature = "stateful")]
pub mod persistence;
pub mod schema;
pub mod versioning;

#[cfg(feature = "stateful")]
pub use crate::nimbus_client::*;
pub use config::RemoteSettingsConfig;
pub use enrollment::{EnrolledFeature, EnrollmentStatus};
pub use error::{NimbusError, Result};
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;
pub use matcher::AppContext;
pub use schema::*;

// Exposed for Example only
pub use evaluator::TargetingAttributes;

#[cfg(test)]
mod tests;
