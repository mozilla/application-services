/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use crate::client::telemetry::{AdsTelemetry, ClientOperationEvent};
use crate::error::{RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError};
use crate::http_cache::{CacheOutcome, HttpCacheBuilderError};
use crate::telemetry::Telemetry;

#[uniffi::export]
pub trait MozAdsTelemetry: Send + Sync {
    fn record_build_cache_error(&self, label: &str, value: &str);
    fn record_client_error(&self, label: &str, value: &str);
    fn record_client_operation_total(&self, label: &str);
    fn record_deserialization_error(&self, label: &str, value: &str);
    fn record_http_cache_outcome(&self, label: &str);
}

pub struct MozAdsTelemetryWrapper {
    pub inner: Arc<dyn MozAdsTelemetry>,
}

impl Telemetry<CacheOutcome> for MozAdsTelemetryWrapper {
    fn record(&self, event: &CacheOutcome) {
        let label = match event {
            CacheOutcome::Hit => "hit",
            CacheOutcome::LookupFailed(_) => "lookup_failed",
            CacheOutcome::NoCache => "no_cache",
            CacheOutcome::MissNotCacheable => "miss_not_cacheable",
            CacheOutcome::MissStored => "miss_stored",
            CacheOutcome::StoreFailed(_) => "store_failed",
            CacheOutcome::CleanupFailed(_) => "cleanup_failed",
        };
        self.inner.record_http_cache_outcome(label);
    }
}

impl Telemetry<serde_json::Error> for MozAdsTelemetryWrapper {
    fn record(&self, event: &serde_json::Error) {
        let label = "invalid_ad_item";
        let value = format!("{}", event);
        self.inner.record_deserialization_error(label, &value);
    }
}

impl Telemetry<HttpCacheBuilderError> for MozAdsTelemetryWrapper {
    fn record(&self, event: &HttpCacheBuilderError) {
        let label = match event {
            HttpCacheBuilderError::EmptyDbPath => "empty_db_path",
            HttpCacheBuilderError::Database(_) => "database_error",
            HttpCacheBuilderError::InvalidMaxSize { .. } => "invalid_max_size",
            HttpCacheBuilderError::InvalidTtl { .. } => "invalid_ttl",
        };
        let value = format!("{}", event);
        self.inner.record_build_cache_error(label, &value);
    }
}

impl Telemetry<RequestAdsError> for MozAdsTelemetryWrapper {
    fn record(&self, event: &RequestAdsError) {
        let label = "request_ads";
        let value = format!("{}", event);
        self.inner.record_client_error(label, &value);
    }
}

impl Telemetry<RecordClickError> for MozAdsTelemetryWrapper {
    fn record(&self, event: &RecordClickError) {
        let label = "record_click";
        let value = format!("{}", event);
        self.inner.record_client_error(label, &value);
    }
}

impl Telemetry<RecordImpressionError> for MozAdsTelemetryWrapper {
    fn record(&self, event: &RecordImpressionError) {
        let label = "record_impression";
        let value = format!("{}", event);
        self.inner.record_client_error(label, &value);
    }
}

impl Telemetry<ReportAdError> for MozAdsTelemetryWrapper {
    fn record(&self, event: &ReportAdError) {
        let label = "report_ad";
        let value = format!("{}", event);
        self.inner.record_client_error(label, &value);
    }
}

impl Telemetry<ClientOperationEvent> for MozAdsTelemetryWrapper {
    fn record(&self, event: &ClientOperationEvent) {
        let label = match event {
            ClientOperationEvent::New => "new",
            ClientOperationEvent::RecordClick => "record_click",
            ClientOperationEvent::RecordImpression => "record_impression",
            ClientOperationEvent::ReportAd => "report_ad",
            ClientOperationEvent::RequestAds => "request_ads",
        };
        self.inner.record_client_operation_total(label);
    }
}

impl AdsTelemetry for MozAdsTelemetryWrapper {}
