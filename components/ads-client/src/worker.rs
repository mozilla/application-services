use crate::{
    client::error::ComponentError,
    http_cache::CachePolicy,
    mars::{
        ad_request::{AdPlacementRequest, AdRequestFlags},
        ad_response::AdSpoc,
        error::CallbackRequestError,
    },
    AdsClientApiResult, MozAdsCallbackOptions, MozAdsClientApiError, MozAdsClientInner,
    MozAdsImage, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount, MozAdsReportReason,
    MozAdsRequestOptions, MozAdsSpoc, MozAdsTile,
};
use error_support::handle_error;
use std::{collections::HashMap, sync::mpsc::Receiver, thread::JoinHandle};
use url::Url as AdsClientUrl;

pub const ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE: usize = 1000;
pub const ADS_CLIENT_WORKER_THREAD_NAME : &'static str = "ads-client.worker";

// Spawn worker thread with reference to client and dispatch command receiver.
// Returns None if thread fails to build.
pub fn worker_thread(inner_client: MozAdsClientInner, rx: Receiver<DispatchCommand>) -> Option<JoinHandle<()>> {
    let worker_thread_handle = std::thread::Builder::new()
        .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
        .spawn(move || crate::worker::worker(inner_client, rx)).inspect_err(|err| {
            error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
        }).ok()?;
    Some(worker_thread_handle)
}

fn worker(inner_client: MozAdsClientInner, rx: Receiver<DispatchCommand>) {
    while let Ok(task) = rx.recv() {

        // Synchronously run tasks in the order they are passed in this separate channel.
        let task: DispatchCommand = task;
        task.run_command(&inner_client)
            .expect("TODO: handle error here. maybe retries should be here?");
    }
}

pub enum DispatchCommand {
    RecordClick {
        click_url: String,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    },
    RecordImpression {
        impression_url: String,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    },
    ReportAd {
        report_url: String,
        reason: MozAdsReportReason,
        options: Option<MozAdsCallbackOptions>,
        callback: Box<dyn ErrorOnlyRequestCallback>,
    },
    RequestImageAds {
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn ImageRequestCallback>,
    },
    RequestSpocAds {
        moz_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn SpocRequestCallback>,
    },
    RequestTileAd {
        moz_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Box<dyn TileRequestCallback>,
    },
}

impl DispatchCommand {
    #[handle_error(ComponentError)]
    pub fn run_command(self, ads_client_inner: &MozAdsClientInner) -> AdsClientApiResult<()> {
        // TODO: Duplicated behavior with the sync functions. Behavior is pretty simple though- probably OK to duplicate.
        match self {
            DispatchCommand::RecordClick {
                click_url,
                options,
                callback,
            } => {
                let resp = (|| {
                    let url = AdsClientUrl::parse(&click_url).map_err(|e| {
                        ComponentError::RecordClick(CallbackRequestError::InvalidUrl(e).into())
                    })?;
                    let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
                    let inner = ads_client_inner.lock();
                    inner
                        .record_click(url, ohttp)
                        .map_err(ComponentError::RecordClick)
                })();

                match resp {
                    Ok(_) => callback.on_success(),
                    Err(e) => callback.on_error(e.into()),
                }
            }
            DispatchCommand::RecordImpression {
                impression_url,
                options,
                callback,
            } => {
                let resp = (|| {
                    let url = AdsClientUrl::parse(&impression_url).map_err(|e| {
                        ComponentError::RecordImpression(CallbackRequestError::InvalidUrl(e).into())
                    })?;
                    let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
                    let inner = ads_client_inner.lock();
                    inner
                        .record_impression(url, ohttp)
                        .map_err(ComponentError::RecordImpression)
                })();

                match resp {
                    Ok(_) => callback.on_success(),
                    Err(e) => callback.on_error(e.into()),
                }
            }
            DispatchCommand::ReportAd {
                report_url,
                reason,
                options,
                callback,
            } => {
                let resp = (|| {
                    let url = AdsClientUrl::parse(&report_url).map_err(|e| {
                        ComponentError::ReportAd(CallbackRequestError::InvalidUrl(e).into())
                    })?;
                    let ohttp = options.map(|o| o.ohttp).unwrap_or(false);
                    let inner = ads_client_inner.lock();
                    inner
                        .report_ad(url, reason.into(), ohttp)
                        .map_err(ComponentError::ReportAd)
                })();

                match resp {
                    Ok(_) => callback.on_success(),
                    Err(e) => callback.on_error(e.into()),
                }
            }
            DispatchCommand::RequestImageAds {
                moz_ad_requests,
                options,
                callback,
            } => {
                let resp = (|| {
                    let inner = ads_client_inner.lock();
                    let requests: Vec<AdPlacementRequest> =
                        moz_ad_requests.iter().map(|r| r.into()).collect();
                    let options = options.unwrap_or_default();
                    let flags = AdRequestFlags::from(&options);
                    let ohttp = options.ohttp;
                    let cache_policy: CachePolicy = options.into();
                    let response = inner
                        .request_image_ads(requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
                })();
                match resp {
                    Ok(ads) => callback.on_ad(ads),
                    Err(e) => callback.on_error(e),
                }
            }

            DispatchCommand::RequestSpocAds {
                moz_ad_requests,
                options,
                callback,
            } => {
                let resp = (|| {
                    let inner = ads_client_inner.lock();
                    let requests: Vec<AdPlacementRequest> =
                        moz_ad_requests.iter().map(|r| r.into()).collect();
                    let options = options.unwrap_or_default();
                    let flags = AdRequestFlags::from(&options);
                    let ohttp = options.ohttp;
                    let cache_policy: CachePolicy = options.into();
                    let response: HashMap<String, Vec<AdSpoc>> = inner
                        .request_spoc_ads(requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    Ok::<_, ComponentError>(
                        response
                            .into_iter()
                            .map(|(k, v)| (k, v.into_iter().map(|spoc| spoc.into()).collect()))
                            .collect(),
                    )
                })();
                match resp {
                    Ok(ads) => callback.on_ad(ads),
                    Err(e) => callback.on_error(e.into()),
                }
            }

            DispatchCommand::RequestTileAd {
                moz_ad_requests,
                options,
                callback,
            } => {
                let resp = (|| {
                    let inner = ads_client_inner.lock();
                    let requests: Vec<AdPlacementRequest> =
                        moz_ad_requests.iter().map(|r| r.into()).collect();
                    let options = options.unwrap_or_default();
                    let flags = AdRequestFlags::from(&options);
                    let ohttp = options.ohttp;
                    let cache_policy: CachePolicy = options.into();
                    let response = inner
                        .request_tile_ads(requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                    Ok(response.into_iter().map(|(k, v)| (k, v.into())).collect())
                })();
                match resp {
                    Ok(tiles) => callback.on_ad(tiles),
                    Err(e) => callback.on_error(e),
                }
            }
        }
        Ok(())
    }
}

// TODO: Uniffi does not currently support generics here or direct function callbacks, so we use a few distinct interfaces.
// TODO: We use #[uniffi::export(callback_interface)] rather than #[uniffi::export(with_foreign)] for reasons discussed here:
// - https://github.com/mozilla/application-services/pull/7443
// In the future, this should be changed to uniffi::export(impl = "foreign") alongside the enum change, when uniffi = 0.32
#[uniffi::export(callback_interface)]
pub trait ErrorOnlyRequestCallback: Send + Sync {
    fn on_success(&self);
    fn on_error(&self, err: MozAdsClientApiError);
}

#[uniffi::export(callback_interface)]
pub trait ImageRequestCallback: Send + Sync {
    fn on_ad(&self, ads: HashMap<String, MozAdsImage>);
    fn on_error(&self, err: MozAdsClientApiError);
}

#[uniffi::export(callback_interface)]
pub trait SpocRequestCallback: Send + Sync {
    fn on_ad(&self, ads: HashMap<String, Vec<MozAdsSpoc>>);
    fn on_error(&self, err: MozAdsClientApiError);
}

#[uniffi::export(callback_interface)]
pub trait TileRequestCallback: Send + Sync {
    fn on_ad(&self, tiles: HashMap<String, MozAdsTile>);
    fn on_error(&self, err: MozAdsClientApiError);
}
