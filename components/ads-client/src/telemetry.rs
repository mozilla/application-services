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
pub use backend::Telemetry;
pub(crate) use backend::{install, uninstall};

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

/// A label of `ads_client.build_cache_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildCacheErrorLabel {
    DatabaseError,
    EmptyDbPath,
    InvalidMaxSize,
    InvalidTtl,
    // TODO: these are not in `metrics.yaml`, so Glean records them as
    // `__other__`. Give the ads store its own metric.
    #[cfg(feature = "stateful")]
    StoreDatabaseError,
    #[cfg(feature = "stateful")]
    StoreEmptyDbPath,
    #[cfg(feature = "stateful")]
    StoreInvalidMaxSize,
}

impl BuildCacheErrorLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::DatabaseError => "database_error",
            Self::EmptyDbPath => "empty_db_path",
            Self::InvalidMaxSize => "invalid_max_size",
            Self::InvalidTtl => "invalid_ttl",
            #[cfg(feature = "stateful")]
            Self::StoreDatabaseError => "store_database_error",
            #[cfg(feature = "stateful")]
            Self::StoreEmptyDbPath => "store_empty_db_path",
            #[cfg(feature = "stateful")]
            Self::StoreInvalidMaxSize => "store_invalid_max_size",
        }
    }
}

/// A label of `ads_client.deserialization_error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeserializationErrorLabel {
    InvalidAdItem,
}

impl DeserializationErrorLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidAdItem => "invalid_ad_item",
        }
    }
}

/// A label of `ads_client.http_cache_outcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpCacheOutcomeLabel {
    CleanupFailed,
    Hit,
    LookupFailed,
    MissNotCacheable,
    MissStored,
    NoCache,
    StoreFailed,
    TrimFailed,
}

impl HttpCacheOutcomeLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::CleanupFailed => "cleanup_failed",
            Self::Hit => "hit",
            Self::LookupFailed => "lookup_failed",
            Self::MissNotCacheable => "miss_not_cacheable",
            Self::MissStored => "miss_stored",
            Self::NoCache => "no_cache",
            Self::StoreFailed => "store_failed",
            Self::TrimFailed => "trim_failed",
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
    backend::client_error(operation.label(), sanitize(error.to_string()));
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the HTTP cache against
/// `ads_client.build_cache_error`.
pub fn record_build_cache_error(error: &HttpCacheBuilderError) {
    let label = match error {
        HttpCacheBuilderError::Database(_) => BuildCacheErrorLabel::DatabaseError,
        HttpCacheBuilderError::EmptyDbPath => BuildCacheErrorLabel::EmptyDbPath,
        HttpCacheBuilderError::InvalidMaxSize { .. } => BuildCacheErrorLabel::InvalidMaxSize,
        HttpCacheBuilderError::InvalidTtl { .. } => BuildCacheErrorLabel::InvalidTtl,
    };
    backend::build_cache_error(label.as_str(), sanitize(error.to_string()));
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records a failure to build the ads store against
/// `ads_client.build_cache_error`.
#[cfg(feature = "stateful")]
pub fn record_ads_store_error(error: &crate::ads_store::builder::AdsStoreBuilderError) {
    // By design: telemetry owns every label, even though the ads store
    // records through it.
    use crate::ads_store::builder::AdsStoreBuilderError;

    let label = match error {
        AdsStoreBuilderError::Database(_) => BuildCacheErrorLabel::StoreDatabaseError,
        AdsStoreBuilderError::EmptyDbPath => BuildCacheErrorLabel::StoreEmptyDbPath,
        AdsStoreBuilderError::InvalidMaxSize { .. } => BuildCacheErrorLabel::StoreInvalidMaxSize,
    };
    backend::build_cache_error(label.as_str(), sanitize(error.to_string()));
}

// By design: a free function, so recording a metric is one call from anywhere.
/// Records the result of an HTTP cache read against
/// `ads_client.http_cache_outcome`.
pub fn record_http_cache_outcome(outcome: &CacheOutcome) {
    let (label, value) = match outcome {
        CacheOutcome::CleanupFailed(e) => (HttpCacheOutcomeLabel::CleanupFailed, e.to_string()),
        CacheOutcome::Hit => (HttpCacheOutcomeLabel::Hit, String::new()),
        CacheOutcome::LookupFailed(e) => (HttpCacheOutcomeLabel::LookupFailed, e.to_string()),
        CacheOutcome::MissNotCacheable => (HttpCacheOutcomeLabel::MissNotCacheable, String::new()),
        CacheOutcome::MissStored => (HttpCacheOutcomeLabel::MissStored, String::new()),
        CacheOutcome::NoCache => (HttpCacheOutcomeLabel::NoCache, String::new()),
        CacheOutcome::StoreFailed(e) => (HttpCacheOutcomeLabel::StoreFailed, e.to_string()),
        CacheOutcome::TrimFailed(e) => (HttpCacheOutcomeLabel::TrimFailed, e.to_string()),
    };
    backend::http_cache_outcome(label.as_str(), sanitize(value));
}

/// Records an ad item we could not deserialize against
/// `ads_client.deserialization_error`.
pub fn record_invalid_ad_item(error: &serde_json::Error) {
    backend::deserialization_error(
        DeserializationErrorLabel::InvalidAdItem.as_str(),
        sanitize(error.to_string()),
    );
}

/// Glean truncates longer string values and records an `invalid_overflow`
/// error (bug 2047640).
const MAX_STRING_BYTES: usize = 255;

/// Makes an error message fit to record: URLs lose their query and fragment,
/// which carry per-request data, and the result fits in a Glean string.
fn sanitize(message: String) -> String {
    let mut sanitized = strip_url_queries(&message);
    if sanitized.len() > MAX_STRING_BYTES {
        let mut end = MAX_STRING_BYTES;
        while !sanitized.is_char_boundary(end) {
            end -= 1;
        }
        sanitized.truncate(end);
    }
    sanitized
}

fn strip_url_queries(message: &str) -> String {
    let mut stripped = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(scheme_end) = rest.find("://") {
        let (before, url_and_rest) = rest.split_at(scheme_end);
        stripped.push_str(before);
        let url_end = url_and_rest
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '"' | '\'' | '>'))
            .unwrap_or(url_and_rest.len());
        let url = &url_and_rest[..url_end];
        let query_start = url.find(['?', '#']).unwrap_or(url.len());
        stripped.push_str(&url[..query_start]);
        rest = &url_and_rest[url_end..];
    }
    stripped.push_str(rest);
    stripped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_strips_url_queries_and_fragments() {
        assert_eq!(
            sanitize(
                "error sending request for url (https://ads.mozilla.org/v1/t?id=1&cap_key=2)"
                    .into()
            ),
            "error sending request for url (https://ads.mozilla.org/v1/t)"
        );
        assert_eq!(
            sanitize(r#"invalid value: string "https://a.test/c#frag", expected a URL"#.into()),
            r#"invalid value: string "https://a.test/c", expected a URL"#
        );
        assert_eq!(
            sanitize("https://a.test/?x=1 then https://b.test/p?y=2".into()),
            "https://a.test/ then https://b.test/p"
        );
        assert_eq!(sanitize("no url here".into()), "no url here");
    }

    #[test]
    fn test_sanitize_truncates_on_a_char_boundary() {
        let sanitized = sanitize("é".repeat(200));
        assert_eq!(sanitized.len(), 254);
        assert!(sanitized.chars().all(|c| c == 'é'));

        assert_eq!(sanitize("a".repeat(300)).len(), MAX_STRING_BYTES);
    }
}
