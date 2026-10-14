/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The telemetry backend: one function per metric in `metrics.yaml`, each
//! forwarded to the process-wide [`Telemetry`] implementation.
//!
//! The function signatures match the glean-sym backend this will be replaced
//! with, so the swap only touches this file. [`Telemetry`], [`install`] and
//! [`uninstall`] exist only because the FFI layer hands us a UniFFI callback
//! that has to be dropped again before shutdown; glean-sym needs none of them.
//!
//! Until a callback is installed, and after it is uninstalled, recording is a
//! no-op.

use std::sync::Arc;

use parking_lot::RwLock;

/// Receives the ads client's metrics.
pub trait Telemetry: Send + Sync {
    fn record_build_cache_error(&self, label: &str, value: String);
    fn record_client_error(&self, label: &str, value: String);
    fn record_client_operation_total(&self, label: &str);
    fn record_deserialization_error(&self, label: &str, value: String);
    fn record_http_cache_outcome(&self, label: &str, value: String);
}

// By design: telemetry is a process-wide singleton, as it will be with glean-sym.
static CALLBACK: RwLock<Option<Arc<dyn Telemetry>>> = RwLock::new(None);

/// Makes `callback` the destination of every metric recorded from now on.
pub fn install(callback: impl Telemetry + 'static) {
    // Dropping a UniFFI callback calls into foreign code, so let the replaced
    // one drop after the lock is released.
    let _replaced = CALLBACK.write().replace(Arc::new(callback));
}

/// Drops the installed callback, after which recording is a no-op.
///
/// UniFFI callbacks left hanging at shutdown crash Firefox Desktop on quit, so
/// the client uninstalls the callback when it shuts down.
pub fn uninstall() {
    // As in `install`, the removed callback drops after the lock is released.
    let _removed = CALLBACK.write().take();
}

fn with_callback(f: impl FnOnce(&dyn Telemetry)) {
    // Clone out of the lock so a slow callback never holds up `install` or
    // `uninstall`.
    let Some(callback) = CALLBACK.read().clone() else {
        return;
    };
    f(callback.as_ref());
}

pub(super) fn build_cache_error(label: &str, value: String) {
    with_callback(|cb| cb.record_build_cache_error(label, value));
}

pub(super) fn client_error(label: &str, value: String) {
    with_callback(|cb| cb.record_client_error(label, value));
}

pub(super) fn client_operation_total(label: &str) {
    with_callback(|cb| cb.record_client_operation_total(label));
}

pub(super) fn deserialization_error(label: &str, value: String) {
    with_callback(|cb| cb.record_deserialization_error(label, value));
}

pub(super) fn http_cache_outcome(label: &str, value: String) {
    with_callback(|cb| cb.record_http_cache_outcome(label, value));
}

/// Serializes tests that install a callback, since they all share the one
/// singleton.
#[cfg(test)]
pub fn test_lock() -> parking_lot::MutexGuard<'static, ()> {
    static LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    LOCK.lock()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{record_client_operation, ClientOperation};
    use parking_lot::Mutex;

    #[derive(Clone, Default)]
    struct RecordingTelemetry(Arc<Mutex<Vec<String>>>);

    impl Telemetry for RecordingTelemetry {
        fn record_build_cache_error(&self, _label: &str, _value: String) {}
        fn record_client_error(&self, _label: &str, _value: String) {}
        fn record_client_operation_total(&self, label: &str) {
            self.0.lock().push(label.to_string());
        }
        fn record_deserialization_error(&self, _label: &str, _value: String) {}
        fn record_http_cache_outcome(&self, _label: &str, _value: String) {}
    }

    #[test]
    fn test_records_to_installed_callback() {
        let _lock = test_lock();
        let recording = RecordingTelemetry::default();
        install(recording.clone());

        record_client_operation(ClientOperation::ReportAd);
        // Other tests may record concurrently, so only look for ours.
        assert!(recording.0.lock().iter().any(|label| label == "report_ad"));

        uninstall();
        recording.0.lock().clear();
        record_client_operation(ClientOperation::ReportAd);
        assert!(recording.0.lock().is_empty());
    }
}
