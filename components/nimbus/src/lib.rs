// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

extern crate core;

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

pub use enrollment::{EnrolledFeature, EnrollmentStatus};
pub use error::{NimbusError, Result};
#[cfg(debug_assertions)]
pub use evaluator::evaluate_enrollment;
pub use schema::*;
pub use targeting::NimbusTargetingHelper;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {

        pub mod stateful;

        pub use stateful::nimbus_client::*;
        pub use stateful::matcher::AppContext;
        pub use remote_settings::{RemoteSettingsConfig, RemoteSettingsServer};
    } else {
        pub mod stateless;

        pub use stateless::cirrus_client::*;
        pub use stateless::matcher::AppContext;
    }
}

// Exposed for Example only
pub use evaluator::TargetingAttributes;

pub(crate) const SLUG_REPLACEMENT_PATTERN: &str = "{experiment}";

#[cfg(test)]
mod tests;
