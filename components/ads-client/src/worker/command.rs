use std::{collections::HashMap, sync::mpsc::SyncSender};

use error_support::handle_error;
use url::Url;

use crate::{
    client::{
        error::{BackgroundWorkerError, ComponentError},
        CommandDispatchedOperationEvent, CommandFailedOperationEvent,
        CommandProcessedOperationEvent,
    },
    http_cache::CachePolicy,
    mars::{
        ad_request::AdPlacementRequest,
        ad_response::{AdImage, AdSpoc, AdTile},
        ReportReason,
    },
    telemetry::Telemetry,
    AdsClientApiResult, MozAdsClientInner, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
};

pub enum DispatchCommand {
    RequestImageAds {
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
    },
    RequestSpocAds {
        spoc_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
    },
    RequestTileAds {
        tile_ad_requests: Vec<MozAdsPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
    },
    RecordClick {
        url: Url,
        ohttp: bool,
    },
    RecordImpression {
        url: Url,
        ohttp: bool,
    },
    ReportAd {
        url: Url,
        reason: ReportReason,
        ohttp: bool,
    },
    Ping(SyncSender<()>),
}

impl DispatchCommand {
    // Runs a dispatched command synchronously in it's thread.
    // The dispatched command calls the corresponding `AdsClient` synchronous method, meaning that behavior between the two is shared.
    // This includes telemetry calls, meaning that for a successful `RecordClick`, all of the following will get logged:
    // - CommandDispatchedOperationEvent::RecordClick  (on dispatch)
    // - ClientOperationEvent::RecordClick (on `AdsClient` method success)
    // - CommandProcessedOperationEvent::RecordClick (on process)
    #[handle_error(ComponentError)]
    pub fn run_command<T: Telemetry>(
        self,
        ads_client_inner: &MozAdsClientInner,
        telemetry: &T,
    ) -> AdsClientApiResult<()> {
        match self {
            DispatchCommand::RequestImageAds {
                image_ad_requests,
                cache_policy,
                flags,
                ohttp,
            } => {
                let mut inner = ads_client_inner.lock();

                // Image ads
                if !image_ad_requests.is_empty() {
                    let image_ad_requests: Vec<AdPlacementRequest> =
                        image_ad_requests.iter().map(|r| r.into()).collect();
                    let image_response = inner
                        .request_image_ads(image_ad_requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    inner.cache_ads::<AdImage>(image_response);
                }

                telemetry.record(&CommandProcessedOperationEvent::RequestAds);
                Ok(())
            }
            DispatchCommand::RequestSpocAds {
                spoc_ad_requests,
                cache_policy,
                flags,
                ohttp,
            } => {
                let mut inner = ads_client_inner.lock();

                // Spoc ads
                if !spoc_ad_requests.is_empty() {
                    let spoc_ad_requests: Vec<AdPlacementRequest> =
                        spoc_ad_requests.iter().map(|r| r.into()).collect();
                    let spoc_response = inner
                        .request_spoc_ads(spoc_ad_requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    inner.cache_ads::<AdSpoc>(spoc_response);
                }

                telemetry.record(&CommandProcessedOperationEvent::RequestAds);
                Ok(())
            }
            DispatchCommand::RequestTileAds {
                tile_ad_requests,
                cache_policy,
                flags,
                ohttp,
            } => {
                let mut inner = ads_client_inner.lock();

                // Tile ads
                if !tile_ad_requests.is_empty() {
                    let tile_ad_requests: Vec<AdPlacementRequest> =
                        tile_ad_requests.iter().map(|r| r.into()).collect();
                    let tile_response = inner
                        .request_tile_ads(tile_ad_requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    inner.cache_ads::<AdTile>(tile_response);
                }

                telemetry.record(&CommandProcessedOperationEvent::RequestAds);
                Ok(())
            }
            DispatchCommand::RecordClick { url, ohttp } => {
                let inner = ads_client_inner.lock();
                inner
                    .record_click(url, ohttp)
                    .map_err(ComponentError::RecordClick)?;
                telemetry.record(&CommandProcessedOperationEvent::RecordClick);
                Ok(())
            }
            DispatchCommand::RecordImpression { url, ohttp } => {
                let inner = ads_client_inner.lock();
                inner
                    .record_impression(url, ohttp)
                    .map_err(ComponentError::RecordImpression)?;
                telemetry.record(&CommandProcessedOperationEvent::RecordImpression);
                Ok(())
            }
            DispatchCommand::ReportAd { url, ohttp, reason } => {
                let inner = ads_client_inner.lock();
                inner
                    .report_ad(url, reason, ohttp)
                    .map_err(ComponentError::ReportAd)?;
                telemetry.record(&CommandProcessedOperationEvent::ReportAd);
                Ok(())
            }

            DispatchCommand::Ping(sender) => {
                sender
                    .try_send(())
                    .map_err(|err| BackgroundWorkerError::PongFailure(Box::new(err)))?;
                Ok(())
            }
        }
    }

    pub fn dispatch_telemetry_event(&self) -> Option<CommandDispatchedOperationEvent> {
        match self {
            DispatchCommand::RequestImageAds { .. }
            | DispatchCommand::RequestSpocAds { .. }
            | DispatchCommand::RequestTileAds { .. } => {
                Some(CommandDispatchedOperationEvent::RequestAds)
            }
            DispatchCommand::RecordClick { .. } => {
                Some(CommandDispatchedOperationEvent::RecordClick)
            }
            DispatchCommand::RecordImpression { .. } => {
                Some(CommandDispatchedOperationEvent::RecordImpression)
            }
            DispatchCommand::ReportAd { .. } => Some(CommandDispatchedOperationEvent::ReportAd),
            DispatchCommand::Ping(_) => None,
        }
    }

    pub fn failed_telemetry_event(&self) -> Option<CommandFailedOperationEvent> {
        match self {
            DispatchCommand::RequestImageAds { .. }
            | DispatchCommand::RequestSpocAds { .. }
            | DispatchCommand::RequestTileAds { .. } => {
                Some(CommandFailedOperationEvent::RequestAds)
            }
            DispatchCommand::RecordClick { .. } => Some(CommandFailedOperationEvent::RecordClick),
            DispatchCommand::RecordImpression { .. } => {
                Some(CommandFailedOperationEvent::RecordImpression)
            }
            DispatchCommand::ReportAd { .. } => Some(CommandFailedOperationEvent::ReportAd),
            DispatchCommand::Ping(_) => None,
        }
    }
}
