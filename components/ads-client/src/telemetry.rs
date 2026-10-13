/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Metric recording for the ads client.
//!
//! Telemetry is a process-wide singleton: recording a metric is a plain
//! function call from wherever the interesting thing happened, much like a
//! `tracing` macro. Nothing has to be threaded through the clients, stores or
//! transports.
//!
//! This is the same API the crate will expose once metrics are recorded
//! straight from Rust through [`glean-sym`]. Until then, the backend forwards
//! to the [`Telemetry`] implementation the FFI layer installs, which wraps the
//! callback the consumer hands to `MozAdsClientBuilder::telemetry()`; see
//! `telemetry/backend.rs`. Moving to
//! glean-sym means swapping that backend for one that talks to the generated
//! metrics, and none of the recording sites have to change.
//!
//! Every function here is infallible and silent by design. Telemetry must never
//! change the outcome of the operation it is describing.
//!
//! [`glean-sym`]: https://github.com/mozilla/glean/tree/main/glean-core/glean-sym

use std::fmt::Display;

use crate::http_cache::{CacheOutcome, HttpCacheBuilderError};

mod backend;

#[cfg(test)]
pub(crate) use backend::test_lock;
pub(crate) use backend::Installation;
pub use backend::Telemetry;

/// A client operation, as labeled by `ads_client.client_operation_total` and
/// `ads_client.client_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientOperation {
    New,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestAds,
}

impl ClientOperation {
    fn label(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::RecordClick => "record_click",
            Self::RecordImpression => "record_impression",
            Self::ReportAd => "report_ad",
            Self::RequestAds => "request_ads",
        }
    }
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records an attempted operation against `ads_client.client_operation_total`.
pub fn record_client_operation(operation: ClientOperation) {
    backend::client_operation_total(operation.label());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failed operation against `ads_client.client_error`.
///
/// Errors are recorded here even when they are also propagated to the consumer.
pub fn record_client_error(operation: ClientOperation, error: &impl Display) {
    backend::client_error(operation.label(), error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the HTTP cache against
/// `ads_client.build_cache_error`.
pub fn record_build_cache_error(error: &HttpCacheBuilderError) {
    let label = match error {
        HttpCacheBuilderError::Database(_) => "database_error",
        HttpCacheBuilderError::EmptyDbPath => "empty_db_path",
        HttpCacheBuilderError::InvalidMaxSize { .. } => "invalid_max_size",
        HttpCacheBuilderError::InvalidTtl { .. } => "invalid_ttl",
    };
    backend::build_cache_error(label, error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the ads store under `label`.
///
/// TODO: add a metric for the ads store to `metrics.yaml`. Until then its
/// errors are recorded against `ads_client.build_cache_error`, under
/// `store_`-prefixed labels.
#[cfg(feature = "stateful")]
pub fn record_ads_store_error(label: &str, error: &impl Display) {
    backend::build_cache_error(label, error.to_string());
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records the result of an HTTP cache read against
/// `ads_client.http_cache_outcome`.
pub fn record_http_cache_outcome(outcome: &CacheOutcome) {
    let (label, value) = match outcome {
        CacheOutcome::CleanupFailed(e) => ("cleanup_failed", e.to_string()),
        CacheOutcome::Hit => ("hit", String::new()),
        CacheOutcome::LookupFailed(e) => ("lookup_failed", e.to_string()),
        CacheOutcome::MissNotCacheable => ("miss_not_cacheable", String::new()),
        CacheOutcome::MissStored => ("miss_stored", String::new()),
        CacheOutcome::NoCache => ("no_cache", String::new()),
        CacheOutcome::StoreFailed(e) => ("store_failed", e.to_string()),
        CacheOutcome::TrimFailed(e) => ("trim_failed", e.to_string()),
    };
    backend::http_cache_outcome(label, value);
}

/// Records an ad item we could not deserialize against
/// `ads_client.deserialization_error`.
pub fn record_invalid_ad_item(error: &serde_json::Error) {
    backend::deserialization_error("invalid_ad_item", error.to_string());
}
