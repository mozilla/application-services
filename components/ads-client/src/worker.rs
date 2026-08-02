use crate::{
    AdsClientApiResult, MozAdsClientApiError, MozAdsClientInner, MozAdsPlacementRequest, MozAdsRequestOptions, client::error::ComponentError, http_cache::CachePolicy, mars::{
        ad_request::{AdPlacementRequest, AdRequestFlags}, ad_response::AdImage,
    }
};
use error_support::{convert_log_report_error, handle_error};
use std::{collections::HashMap, sync::mpsc::{self, Receiver, SyncSender}, thread::JoinHandle};

pub const ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE: usize = 1000;
pub const ADS_CLIENT_WORKER_THREAD_NAME : &'static str = "ads-client.worker";

// Spawn worker thread from a reference to the client, returning a synchronous channel transmitter to the thread, and its JoinHandle.
// Returns None if thread fails to build.
pub fn build_worker_thread(inner_client: MozAdsClientInner) -> Option<(SyncSender<DispatchCommand>, JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel(ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE);
    let worker_thread_handle = std::thread::Builder::new()
        .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
        .spawn(move || crate::worker::worker(inner_client, rx)).inspect_err(|err| {
            error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
        }).ok()?;
    Some((tx, worker_thread_handle))
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
    // TODO: Extend this with other possible commands in V2.
    RequestImageAds {
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
        callback: Option<Box<dyn ErrorOnlyRequestCallback>>
    }
}

impl DispatchCommand {
    #[handle_error(ComponentError)]
    pub fn run_command(self, ads_client_inner: &MozAdsClientInner) -> AdsClientApiResult<()> {
        // TODO: Duplicated behavior with the sync functions. Behavior is pretty simple though- probably OK to duplicate.
        match self {
            DispatchCommand::RequestImageAds {
                image_ad_requests,
                options,
                callback
            } => {
                let resp = (|| {
                    let mut inner = ads_client_inner.lock();
                    let options = options.unwrap_or_default();
                    let flags = AdRequestFlags::from(&options);
                    let ohttp = options.ohttp;
                    let cache_policy: CachePolicy = options.into();

                    // Image ads
                    if image_ad_requests.len() > 0 {
                    let image_ad_requests: Vec<AdPlacementRequest> =
                        image_ad_requests.iter().map(|r| r.into()).collect();
                    let image_response = inner
                        .request_image_ads(image_ad_requests, flags, Some(cache_policy), ohttp)
                        .map_err(ComponentError::RequestAds)?;
                        let image_response: HashMap<String, Vec<AdImage>> = image_response.into_iter().map(|(k, v)| (k, vec![v.into()])).collect();
                        inner.cache_ads(image_response);
                    }
                    Ok::<(), ComponentError>(())
                })();
                match resp {
                    Ok(_) => (),
                    Err(e) => handle_background_worker_error(e, callback),
                }
            }
        }
        Ok(())
    }
}

// Handles an error thrown by the background worker by converting it to public-facing error type (MozAdsClientApiError) and logging it.
// If a callback was provided, public error gets sent there too.
fn handle_background_worker_error(err : ComponentError, callback : Option<Box<dyn ErrorOnlyRequestCallback>>) {
    let err : MozAdsClientApiError = convert_log_report_error(err);
    if let Some(callback) = callback {
        callback.on_error(err);
    }
}

// TODO: Uniffi does not currently support direct function callbacks, so we use an interface.
// TODO: We use #[uniffi::export(callback_interface)] rather than #[uniffi::export(with_foreign)] for reasons discussed here:
// - https://github.com/mozilla/application-services/pull/7443
// In the future, this should be changed to uniffi::export(impl = "foreign") alongside the enum change, when uniffi = 0.32
#[uniffi::export(callback_interface)]
pub trait ErrorOnlyRequestCallback: Send + Sync {
    fn on_error(&self, err: MozAdsClientApiError);
}

