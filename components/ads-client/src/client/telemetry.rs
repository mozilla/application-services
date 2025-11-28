use std::fmt::Debug;

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/
use crate::error::{RecordClickError, RecordImpressionError, ReportAdError, RequestAdsError};
use crate::http_cache::HttpCacheBuilderError;
use crate::mars::MARSTelemetry;
use crate::telemetry::Telemetry;

pub trait AdsTelemetry:
    MARSTelemetry
    + Telemetry<ClientOperationEvent>
    + Telemetry<HttpCacheBuilderError>
    + Telemetry<RecordClickError>
    + Telemetry<RecordImpressionError>
    + Telemetry<ReportAdError>
    + Telemetry<RequestAdsError>
{
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientOperationEvent {
    New,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestAds,
}

pub struct PrintAdsTelemetry;

impl<A: Debug> Telemetry<A> for PrintAdsTelemetry {
    fn record(&self, event: &A) {
        println!("record: {:?}", event);
    }
}

impl AdsTelemetry for PrintAdsTelemetry {}
