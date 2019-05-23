/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::{Error, ErrorKind, StorageHttpError};
use crate::telemetry::SyncTelemetryPing;
use std::collections::HashMap;

/// The general status of sync - should probably be moved to the "sync manager"
/// once we have one!
#[derive(Debug, PartialEq)]
pub enum ServiceStatus {
    /// Everything is fine.
    Ok,
    /// Some general network issue.
    NetworkError,
    /// Some apparent issue with the servers.
    ServiceError,
    /// Some external FxA action needs to be taken.
    AuthenticationError,
    /// We declined to do anything for backoff or rate-limiting reasons.
    BackedOff,
    /// We were interrupted.
    Interrupted,
    /// Something else - you need to check the logs for more details. May
    /// or may not be transient, we really don't know.
    OtherError,
}

impl ServiceStatus {
    // This is a bit naive and probably will not survive in this form in the
    // SyncManager - eg, we'll want to handle backoff etc.
    pub fn from_err(err: &Error) -> ServiceStatus {
        match err.kind() {
            // HTTP based errors.
            ErrorKind::TokenserverHttpError(status) => {
                // bit of a shame the tokenserver is different to storage...
                if *status == 401 {
                    ServiceStatus::AuthenticationError
                } else {
                    ServiceStatus::ServiceError
                }
            }
            // BackoffError is also from the tokenserver.
            ErrorKind::BackoffError(_) => ServiceStatus::ServiceError,
            ErrorKind::StorageHttpError(ref e) => match e {
                StorageHttpError::Unauthorized { .. } => ServiceStatus::AuthenticationError,
                _ => ServiceStatus::ServiceError,
            },

            // Network errors.
            ErrorKind::OpensslError(_)
            | ErrorKind::RequestError(_)
            | ErrorKind::UnexpectedStatus(_)
            | ErrorKind::HawkError(_) => ServiceStatus::NetworkError,

            ErrorKind::Interrupted(_) => ServiceStatus::Interrupted,
            _ => ServiceStatus::OtherError,
        }
    }
}

/// The result of a sync request. This too is from the "sync manager", but only
/// has a fraction of the things it will have when we actually build that.
#[derive(Debug)]
pub struct SyncResult {
    /// The general health.
    pub service_status: ServiceStatus,

    /// The result of the sync.
    pub result: Result<(), Error>,

    /// The result for each engine.
    /// Note that we expect the `String` to be replaced with an enum later.
    pub engine_results: HashMap<String, Result<(), Error>>,

    pub telemetry: SyncTelemetryPing,
}
