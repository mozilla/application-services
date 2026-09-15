/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[cfg(feature = "stateful")]
use std::sync::mpsc::{RecvTimeoutError, TrySendError};

use crate::mars::error::{FetchAdsError, RecordClickError, RecordImpressionError, ReportAdError};
#[cfg(feature = "stateful")]
use crate::worker::command;

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[cfg(feature = "stateful")]
    #[error("Error requesting ads from worker: {0}")]
    BackgroundWorker(#[from] BackgroundWorkerError),

    #[error("Error recording a click for a placement: {0}")]
    RecordClick(#[from] RecordClickError),

    #[error("Error recording an impressions for a placement: {0}")]
    RecordImpression(#[from] RecordImpressionError),

    #[error("Error reporting an ad: {0}")]
    ReportAd(#[from] ReportAdError),

    #[error("Error requesting ads: {0}")]
    RequestAds(#[from] RequestAdsError),
}

#[derive(Debug, thiserror::Error)]
pub enum RequestAdsError {
    #[error(transparent)]
    ContextId(#[from] context_id::ApiError),

    #[error("Error requesting ads from MARS: {0}")]
    FetchAds(#[from] FetchAdsError),
}

#[cfg(feature = "stateful")]
#[derive(Debug, thiserror::Error)]
pub enum BackgroundWorkerError {
    #[error("Error requesting new ads from the background worker: worker closed")]
    Closed,

    #[error("Error requesting new ads from the background worker: worker full")]
    Full,

    #[error("Background worker timed out waiting for response: {0}")]
    TimedOut(#[from] RecvTimeoutError),
}

#[cfg(feature = "stateful")]
impl From<TrySendError<command::DispatchCommand>> for BackgroundWorkerError {
    fn from(value: TrySendError<command::DispatchCommand>) -> Self {
        match value {
            TrySendError::Disconnected(_) => BackgroundWorkerError::Closed,
            TrySendError::Full(_) => BackgroundWorkerError::Full,
        }
    }
}
