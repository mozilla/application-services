/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use crate::error::Error;

/// Remote Settings sync status.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum RemoteSettingsSyncStatus {
    /// Sync completed and new data was stored.
    Success,
    /// Local data is already up to date, no new data was stored.
    UpToDate,
    /// A network-level error occurred (connection refused, timeout, bad HTTP status, ...)
    NetworkError,
    /// The server asked the client to back off.
    BackoffError,
    /// Content signature verification failed.
    SignatureError,
    /// Server error (5xx status)
    ServerError,
    /// An unknown error occurred.
    UnknownError,
}

#[derive(Debug, PartialEq, uniffi::Record, Default)]
#[allow(non_snake_case)]
pub struct SyncStatusExtras {
    /// Duration of the sync operation in milliseconds, if available.
    pub duration: Option<u64>,
    /// The name of the error that occurred, if available.
    pub errorName: Option<String>,
}

/// Trait implemented by consumers to record Remote Settings metrics with Glean.
///
/// Consumers should implement this trait and pass it to
/// [crate::RemoteSettingsService::set_telemetry].
///
/// Consumers implement the trait like this (Kotlin example):
/// ```
/// import mozilla.appservices.remote_settings.RemoteSettingsTelemetry
/// import org.mozilla.appservices.remote_settings.GleanMetrics.RemoteSettingsClient
///
/// class RSTelemetry : RemoteSettingsTelemetry {
///     override fun report(source: String, value: RemoteSettingsSyncStatus, extras: SyncStatusExtras) {
///         RemoteSettingsClient.syncStatus.record(
///             RemoteSettingsClient.SyncStatusExtra(
///                 value = value,
///                 source = source,
///                 duration = extras.duration,
///                 errorName = extras.errorName
///             )
///         )
///     }
/// }
///
/// service.setTelemetry(RSTelemetry())
/// ```
#[uniffi::export(with_foreign)]
pub trait RemoteSettingsTelemetry: Send + Sync {
    /// Report uptake event.
    fn report(&self, source: String, value: RemoteSettingsSyncStatus, extras: SyncStatusExtras);
}

struct NoopRemoteSettingsTelemetry;

impl RemoteSettingsTelemetry for NoopRemoteSettingsTelemetry {
    fn report(&self, _source: String, _value: RemoteSettingsSyncStatus, _extras: SyncStatusExtras) {
    }
}

/// Wrapper around [RemoteSettingsTelemetry] used internally.
#[derive(Clone)]
pub struct RemoteSettingsTelemetryWrapper {
    inner: Arc<dyn RemoteSettingsTelemetry>,
}

impl RemoteSettingsTelemetryWrapper {
    pub fn new(inner: Arc<dyn RemoteSettingsTelemetry>) -> Self {
        Self { inner }
    }

    pub fn noop() -> Self {
        Self {
            inner: Arc::new(NoopRemoteSettingsTelemetry),
        }
    }

    pub fn report_success(&self, source: &str, duration: Option<u64>) {
        self.inner.report(
            source.to_string(),
            RemoteSettingsSyncStatus::Success,
            SyncStatusExtras {
                duration,
                errorName: None,
            },
        );
    }

    pub fn report_up_to_date(&self, source: &str, duration: Option<u64>) {
        self.inner.report(
            source.to_string(),
            RemoteSettingsSyncStatus::UpToDate,
            SyncStatusExtras {
                duration,
                errorName: None,
            },
        );
    }

    pub fn report_sync_error(&self, error: &Error, source: &str) {
        self.inner.report(
            source.to_string(),
            error_to_status(error),
            SyncStatusExtras {
                duration: None,
                errorName: Some(format!("{error:?}")),
            },
        );
    }
}

fn error_to_status(error: &Error) -> RemoteSettingsSyncStatus {
    match error {
        Error::RequestError(viaduct::ViaductError::NetworkError(_))
        | Error::ResponseError { .. } => RemoteSettingsSyncStatus::NetworkError,
        Error::BackoffError(_) => RemoteSettingsSyncStatus::BackoffError,
        #[cfg(feature = "signatures")]
        Error::IncompleteSignatureDataError(_) => RemoteSettingsSyncStatus::SignatureError,
        #[cfg(feature = "signatures")]
        Error::SignatureError(_) => RemoteSettingsSyncStatus::SignatureError,
        _ => RemoteSettingsSyncStatus::UnknownError,
    }
}
