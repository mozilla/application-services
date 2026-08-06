/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::any::Any;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::client::error::RequestAdsError;
use crate::client::ClientOperationEvent;
use crate::http_cache::{CacheOutcome, HttpCacheBuilderError};
use crate::mars::error::{RecordClickError, RecordImpressionError, ReportAdError};
use crate::telemetry::Telemetry;

#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetry: Send + Sync {
    fn record_build_cache_error(&self, label: String, value: String);
    fn record_client_error(&self, label: String, value: String);
    fn record_client_operation_total(&self, label: String);
    fn record_deserialization_error(&self, label: String, value: String);
    fn record_http_cache_outcome(&self, label: String, value: String);
}

pub struct NoopMozAdsTelemetry;

impl MozAdsTelemetry for NoopMozAdsTelemetry {
    fn record_build_cache_error(&self, _label: String, _value: String) {}
    fn record_client_error(&self, _label: String, _value: String) {}
    fn record_client_operation_total(&self, _label: String) {}
    fn record_deserialization_error(&self, _label: String, _value: String) {}
    fn record_http_cache_outcome(&self, _label: String, _value: String) {}
}

#[derive(Clone)]
pub struct MozAdsTelemetryWrapper {
    inner: Arc<RwLock<Option<Arc<dyn MozAdsTelemetry>>>>,
}

impl MozAdsTelemetryWrapper {
    pub fn new(inner: Arc<dyn MozAdsTelemetry>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(inner))),
        }
    }

    pub fn noop() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(Arc::new(NoopMozAdsTelemetry)))),
        }
    }

    #[cfg(test)]
    pub fn clone_inner_arc(&self) -> Option<Arc<dyn MozAdsTelemetry>> {
        self.inner.read().clone()
    }
}

impl Telemetry for MozAdsTelemetryWrapper {
    // MozAdsTelemetry has hanging uniffi callbacks which need to be explicitly dropped before closing.
    // This replaces it with a `None` internally, meaning future calls will be noops.
    fn shutdown(&self) {
        let _dropped = self.inner.write().take();
    }

    fn record(&self, event: &dyn Any) {
        let Some(inner) = self.inner.read().clone() else {
            return;
        };

        if let Some(cache_outcome) = event.downcast_ref::<CacheOutcome>() {
            inner.record_http_cache_outcome(
                match cache_outcome {
                    CacheOutcome::Hit => "hit".to_string(),
                    CacheOutcome::LookupFailed(_) => "lookup_failed".to_string(),
                    CacheOutcome::NoCache => "no_cache".to_string(),
                    CacheOutcome::MissNotCacheable => "miss_not_cacheable".to_string(),
                    CacheOutcome::MissStored => "miss_stored".to_string(),
                    CacheOutcome::StoreFailed(_) => "store_failed".to_string(),
                    CacheOutcome::TrimFailed(_) => "trim_failed".to_string(),
                    CacheOutcome::CleanupFailed(_) => "cleanup_failed".to_string(),
                },
                match cache_outcome {
                    CacheOutcome::LookupFailed(e) => e.to_string(),
                    CacheOutcome::StoreFailed(e) => e.to_string(),
                    CacheOutcome::TrimFailed(e) => e.to_string(),
                    CacheOutcome::CleanupFailed(e) => e.to_string(),
                    _ => "".to_string(),
                },
            );
            return;
        }
        if let Some(client_op) = event.downcast_ref::<ClientOperationEvent>() {
            inner.record_client_operation_total(match client_op {
                ClientOperationEvent::New => "new".to_string(),
                ClientOperationEvent::RecordClick => "record_click".to_string(),
                ClientOperationEvent::RecordImpression => "record_impression".to_string(),
                ClientOperationEvent::ReportAd => "report_ad".to_string(),
                ClientOperationEvent::RequestAds => "request_ads".to_string(),
            });
            return;
        }
        if let Some(cache_builder_error) = event.downcast_ref::<HttpCacheBuilderError>() {
            inner.record_build_cache_error(
                match cache_builder_error {
                    HttpCacheBuilderError::EmptyDbPath => "empty_db_path".to_string(),
                    HttpCacheBuilderError::Database(_) => "database_error".to_string(),
                    HttpCacheBuilderError::InvalidMaxSize { .. } => "invalid_max_size".to_string(),
                    HttpCacheBuilderError::InvalidTtl { .. } => "invalid_ttl".to_string(),
                },
                format!("{}", cache_builder_error),
            );
            return;
        }
        if let Some(record_click_error) = event.downcast_ref::<RecordClickError>() {
            inner.record_client_error(
                "record_click".to_string(),
                format!("{}", record_click_error),
            );
            return;
        }
        if let Some(record_impression_error) = event.downcast_ref::<RecordImpressionError>() {
            inner.record_client_error(
                "record_impression".to_string(),
                format!("{}", record_impression_error),
            );
            return;
        }
        if let Some(report_ad_error) = event.downcast_ref::<ReportAdError>() {
            inner.record_client_error("report_ad".to_string(), format!("{}", report_ad_error));
            return;
        }
        if let Some(request_ads_error) = event.downcast_ref::<RequestAdsError>() {
            inner.record_client_error("request_ads".to_string(), format!("{}", request_ads_error));
            return;
        }
        if let Some(json_error) = event.downcast_ref::<serde_json::Error>() {
            inner.record_deserialization_error(
                "invalid_ad_item".to_string(),
                format!("{}", json_error),
            );
            return;
        }
        eprintln!("Unsupported telemetry event type: {:?}", event.type_id());
        #[cfg(test)]
        panic!("Unsupported telemetry event type: {:?}", event.type_id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_panic_on_unsupported_event() {
        struct UnsupportedEvent;
        let telemetry = MozAdsTelemetryWrapper::noop();
        telemetry.record(&UnsupportedEvent);
    }
}
