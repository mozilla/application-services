/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! The telemetry backend: one function per metric in `metrics.yaml`, each
//! forwarded to the process-wide [`Telemetry`] implementation.
//!
//! The function signatures match the glean-sym backend this will be replaced
//! with, so the swap only touches this file. [`Telemetry`] and
//! [`Installation`] exist only because the FFI layer hands us a UniFFI callback
//! that has to be dropped again before shutdown; glean-sym needs none of them.
//!
//! Until a callback is installed, and after it is uninstalled, recording is a
//! no-op.

use std::sync::{Arc, Weak};

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

/// A callback put in place by [`Installation::install`]. Weak, so that only
/// the singleton keeps the callback alive.
pub struct Installation(Weak<dyn Telemetry>);

impl Installation {
    /// Makes `callback` the destination of every metric recorded from now on,
    /// replacing any callback installed before it.
    pub fn install(callback: impl Telemetry + 'static) -> Self {
        let callback: Arc<dyn Telemetry> = Arc::new(callback);
        let installation = Self(Arc::downgrade(&callback));
        // Dropping a UniFFI callback calls into foreign code, so let the
        // replaced one drop after the lock is released.
        let _replaced = CALLBACK.write().replace(callback);
        installation
    }

    /// Drops the callback, unless another one has replaced it since. Recording
    /// is then a no-op until another one is installed.
    ///
    /// UniFFI callbacks left hanging at shutdown crash Firefox Desktop on quit,
    /// so each client uninstalls its callback when it shuts down.
    pub fn uninstall(&self) {
        // As in `install`, the removed callback drops after the lock is released.
        let _removed = {
            let mut installed = CALLBACK.write();
            let is_ours = installed
                .as_ref()
                .is_some_and(|current| Weak::ptr_eq(&Arc::downgrade(current), &self.0));
            if is_ours {
                installed.take()
            } else {
                None
            }
        };
    }
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
        let installation = Installation::install(recording.clone());

        record_client_operation(ClientOperation::ReportAd);
        // Other tests may record concurrently, so only look for ours.
        assert!(recording.0.lock().iter().any(|label| label == "report_ad"));

        installation.uninstall();
        recording.0.lock().clear();
        record_client_operation(ClientOperation::ReportAd);
        assert!(recording.0.lock().is_empty());
    }

    #[test]
    fn test_uninstall_leaves_a_replacement_alone() {
        let _lock = test_lock();
        let first = Installation::install(RecordingTelemetry::default());
        let second = Installation::install(RecordingTelemetry::default());
        // Replacing a callback drops it.
        assert_eq!(first.0.strong_count(), 0);

        first.uninstall();
        assert_eq!(second.0.strong_count(), 1);

        second.uninstall();
        assert_eq!(second.0.strong_count(), 0);
    }
}
