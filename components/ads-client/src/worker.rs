use crate::{
    client::error::{BackgroundWorkerError, ComponentError},
    http_cache::CachePolicy,
    mars::{
        ad_request::{AdPlacementRequest, AdRequestFlags},
        ad_response::AdImage,
    },
    AdsClientApiResult, MozAdsClientApiError, MozAdsClientInner, MozAdsPlacementRequest,
    MozAdsRequestOptions,
};
use error_support::handle_error;
use std::{
    collections::HashMap,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
};

pub const ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE: usize = 1000;
pub const ADS_CLIENT_WORKER_THREAD_NAME: &'static str = "ads-client.worker";

// Spawn worker thread from a reference to the client, returning a synchronous channel transmitter to the thread, and its JoinHandle.
// Returns None if thread fails to build.
pub fn build_worker_thread(
    inner_client: MozAdsClientInner,
) -> Option<(SyncSender<Dispatch>, JoinHandle<()>)> {
    let (tx, rx) = mpsc::sync_channel(ADS_CLIENT_WORKER_CHANNEL_BUFFER_SIZE);
    let worker_thread_handle = std::thread::Builder::new()
        .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
        .spawn(move || crate::worker::worker(inner_client, rx)).inspect_err(|err| {
            error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
        }).ok()?;
    Some((tx, worker_thread_handle))
}

fn worker(inner_client: MozAdsClientInner, rx: Receiver<Dispatch>) {
    while let Ok(task) = rx.recv() {
        // Synchronously run tasks in the order they are passed in this separate channel.
        let Dispatch {
            command,
            error_callback,
        } = task;

        if let Err(e) = command.run_command(&inner_client) {
            // TODO: document this more clearly
            // Error is logged through `handle_error` conversion macro.
            // If an error callback is provided by the surface, we send the error to that, too.
            if let Some(error_callback) = error_callback {
                error_callback.on_error(e);
            }
        }
    }
}

pub struct Dispatch {
    pub command: DispatchCommand,
    pub error_callback: Option<Box<dyn ErrorOnlyRequestCallback>>,
}

pub enum DispatchCommand {
    // TODO: Extend this with other possible commands in V2.
    RequestImageAds {
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        options: Option<MozAdsRequestOptions>,
    },
    Ping(SyncSender<()>),
}

impl DispatchCommand {
    #[handle_error(ComponentError)]
    pub fn run_command(self, ads_client_inner: &MozAdsClientInner) -> AdsClientApiResult<()> {
        // TODO: Duplicated behavior with the sync functions. Behavior is pretty simple though- probably OK to duplicate.
        match self {
            DispatchCommand::RequestImageAds {
                image_ad_requests,
                options,
            } => {
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
                    let image_response: HashMap<String, Vec<AdImage>> = image_response
                        .into_iter()
                        .map(|(k, v)| (k, vec![v.into()]))
                        .collect();
                    inner.cache_ads(image_response);
                }
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
}

// TODO: Uniffi does not currently support direct function callbacks, so we use an interface.
// TODO: We use #[uniffi::export(callback_interface)] rather than #[uniffi::export(with_foreign)] for reasons discussed here:
// - https://github.com/mozilla/application-services/pull/7443
// In the future, this should be changed to uniffi::export(impl = "foreign") alongside the enum change, when uniffi = 0.32
#[uniffi::export(callback_interface)]
pub trait ErrorOnlyRequestCallback: Send + Sync {
    fn on_error(&self, err: MozAdsClientApiError);
}
