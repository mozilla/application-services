/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use crate::telemetry::Telemetry;

/// Receives the ads client's metrics, for the app to record in Glean.
#[uniffi::export(callback_interface)]
pub trait MozAdsTelemetry: Send + Sync {
    // By design: the foreign callback signature; labels come from `metrics.yaml`.
    fn record_build_cache_error(&self, label: String, value: String);
    // By design: the foreign callback signature; labels come from `metrics.yaml`.
    fn record_client_error(&self, label: String, value: String);
    fn record_client_operation_total(&self, label: String);
    // By design: the foreign callback signature; labels come from `metrics.yaml`.
    fn record_deserialization_error(&self, label: String, value: String);
    // By design: the foreign callback signature; labels come from `metrics.yaml`.
    fn record_http_cache_outcome(&self, label: String, value: String);
}

/// Adapts the consumer's [`MozAdsTelemetry`] callback to the core [`Telemetry`] trait.
pub struct MozAdsTelemetryWrapper(pub Arc<dyn MozAdsTelemetry>);

impl Telemetry for MozAdsTelemetryWrapper {
    fn record_build_cache_error(&self, label: &str, value: String) {
        self.0.record_build_cache_error(label.to_string(), value);
    }

    fn record_client_error(&self, label: &str, value: String) {
        self.0.record_client_error(label.to_string(), value);
    }

    fn record_client_operation_total(&self, label: &str) {
        self.0.record_client_operation_total(label.to_string());
    }

    fn record_deserialization_error(&self, label: &str, value: String) {
        self.0
            .record_deserialization_error(label.to_string(), value);
    }

    fn record_http_cache_outcome(&self, label: &str, value: String) {
        self.0.record_http_cache_outcome(label.to_string(), value);
    }
}

#[cfg(test)]
pub struct NoopMozAdsTelemetry;

#[cfg(test)]
impl MozAdsTelemetry for NoopMozAdsTelemetry {
    fn record_build_cache_error(&self, _label: String, _value: String) {}
    fn record_client_error(&self, _label: String, _value: String) {}
    fn record_client_operation_total(&self, _label: String) {}
    fn record_deserialization_error(&self, _label: String, _value: String) {}
    fn record_http_cache_outcome(&self, _label: String, _value: String) {}
}
