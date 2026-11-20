/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::glean_metrics;

/// Represents an expected failure
///
/// This is used for things that we know will go wrong without any code errors:
/// network errors, disk corruption, etc.
pub enum ExpectedFailure {
    /// Network request timed out
    NetworkTimeout,
    /// General network failure
    Network,
    /// Disk corruption for a SQLite database
    SqliteCorruption,
}

impl ExpectedFailure {
    /// Record an expected failure via the glean counter
    ///
    /// `component_name` must match one of the key labels from the `expected_failures` metric,
    /// defined in components/support/error/metrics.yaml.
    pub fn increment(&self, component_name: impl Into<String>) {
        glean_metrics::rust_component_errors::expected_failures
            .get(component_name.into(), self.category().to_string())
            .add(1);
    }

    /// Glean category from the `expected_failures` metric.
    fn category(&self) -> &'static str {
        match self {
            Self::NetworkTimeout => "network-timeout",
            Self::Network => "network-failure",
            Self::SqliteCorruption => "sqlite-corruption",
        }
    }
}
