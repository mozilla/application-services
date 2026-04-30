/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Exponential backoff settings for retrying transient failures (5xx / network errors).
///
/// Delay after attempt N = `min(initial_delay_ms * 2^N, max_delay_ms)`.
/// Pass `None` in [`WcsConfig::retry_config`] to use the default (3 retries, 1 s → 30 s cap).
#[derive(Clone, Debug, uniffi::Record)]
pub struct WcsRetryConfig {
    /// Maximum number of retry attempts after the initial failure.
    pub max_retries: u32,
    /// Delay before the first retry, in milliseconds.
    pub initial_delay_ms: u64,
    /// Upper bound on the inter-retry delay, in milliseconds.
    pub max_delay_ms: u64,
}

impl WcsRetryConfig {
    pub fn default_config() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1_000,
            max_delay_ms: 30_000,
        }
    }

    /// Zero-retry config used in tests so fakes never sleep.
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            initial_delay_ms: 0,
            max_delay_ms: 0,
        }
    }
}

/// Configuration for the WCS client.
#[derive(Clone, Debug, uniffi::Record)]
pub struct WcsConfig {
    /// The base host for the Merino endpoint. Defaults to the production host if not set.
    pub base_host: Option<String>,
    /// Retry policy for transient failures. Defaults to 3 retries with exponential backoff if `None`.
    pub retry_config: Option<WcsRetryConfig>,
}

/// Options for a live matches request.
#[derive(Clone, Debug, uniffi::Record)]
pub struct WcsLiveOptions {
    /// Filter matches to those where either team's 3-letter key is in this list (e.g. `["BRA", "ARG"]`).
    /// If empty or `None`, all live matches are returned.
    pub teams: Option<Vec<String>>,
}

/// Options for a matches request (`/api/v1/wcs/matches`).
/// All fields are optional — omitted fields use the server's defaults.
#[derive(Clone, Debug, uniffi::Record)]
pub struct WcsMatchesOptions {
    /// Target date in ISO 8601 format (e.g. `"2026-06-15"`). Defaults to today UTC if not set.
    pub date: Option<String>,
    /// Maximum number of matches to return per bucket (previous, current, next).
    pub limit: Option<i32>,
    /// Filter to matches involving these 3-letter team keys (e.g. `["BRA", "ARG"]`).
    /// If empty or `None`, all matches are returned.
    pub teams: Option<Vec<String>>,
}
