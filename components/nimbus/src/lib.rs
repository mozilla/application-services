// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod behavior;
mod dbcache;
mod enrollment;
mod evaluator;
mod client;
mod config;
mod defaults;
mod matcher;
mod sampling;
mod strings;
mod updating;

pub mod error;
pub mod nimbus;
pub mod persistence;
pub mod versioning;

#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;
pub use config::RemoteSettingsConfig;
pub use enrollment::{EnrollmentStatus, EnrolledFeature};
pub use error::{NimbusError, Result};
pub use matcher::AppContext;
pub use nimbus::*;

// Exposed for Example only
pub use evaluator::TargetingAttributes;

#[cfg(test)]
mod tests;
