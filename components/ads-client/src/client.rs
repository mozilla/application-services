/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

#[cfg(feature = "stateful")]
use crate::ads_store::AdsStore;
use crate::common::bytesize::ByteSize;
use crate::http_cache::{CachePolicy, HttpCache};
use crate::mars::ad_request::{AdPlacementRequest, AdRequestFlags};
use crate::mars::ad_response::{AdImage, AdResponse, AdResponseValue, AdSpoc, AdTile};
use crate::mars::error::{RecordClickError, RecordImpressionError, ReportAdError};
use crate::mars::{MARSClient, ReportReason};
#[cfg(feature = "stateful")]
use crate::shutdown::AdsStoreShutdown;
use crate::shutdown::ShutdownReferences;
use crate::telemetry::Telemetry;
use config::AdsClientConfig;
use context_id::ContextIDComponent;
use context_id_deletion::ContextIdDeletionQueue;
use error::RequestAdsError;
#[cfg(feature = "stateful")]
use parking_lot::Mutex;
use std::collections::HashMap;
#[cfg(feature = "stateful")]
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

pub mod config;
mod context_id_deletion;
pub mod error;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;
const DEFAULT_ROTATION_DAYS: u8 = 3;

/// `ContextIDComponent` sends its own plaintext `DELETE /delete_user` on
/// rotation unless `running_in_test_automation` is set. The ads client sends
/// that deletion itself, over OHTTP when the ad request used it (see
/// `ContextIdDeletionQueue`), so the component's request is always disabled.
/// Firefox does the same for its JS-side component in
/// `browser/modules/ContextId.sys.mjs`. See AC-179.
const DISABLE_CONTEXT_ID_COMPONENT_DELETION: bool = true;

pub struct AdsClient<T>
where
    T: Clone + Telemetry,
{
    #[cfg(feature = "stateful")]
    ads_store: Arc<Mutex<Option<AdsStore>>>,
    client: MARSClient<T>,
    context_id_component: ContextIDComponent,
    context_id_deletion_queue: ContextIdDeletionQueue,
    telemetry: T,
}

impl<T> AdsClient<T>
where
    T: Clone + Telemetry,
{
    pub fn new(client_config: AdsClientConfig<T>) -> Self {
        let context_id_deletion_queue = ContextIdDeletionQueue::default();
        let context_id_component = ContextIDComponent::new(
            &Uuid::new_v4().to_string(),
            0,
            DISABLE_CONTEXT_ID_COMPONENT_DELETION,
            Box::new(context_id_deletion_queue.clone()),
        );

        let telemetry = client_config.telemetry;
        let environment = client_config.environment;

        // Configure the cache if a path is provided.
        // Defaults for ttl and cache size are also set if unspecified.
        let http_cache = client_config.cache_config.and_then(|cache_cfg| {
            let default_cache_ttl = Duration::from_secs(
                cache_cfg
                    .default_cache_ttl_seconds
                    .unwrap_or(DEFAULT_TTL_SECONDS),
            );
            let max_cache_size =
                ByteSize::mib(cache_cfg.max_size_mib.unwrap_or(DEFAULT_MAX_CACHE_SIZE_MIB));

            match HttpCache::builder(cache_cfg.db_path)
                .max_size(max_cache_size)
                .default_ttl(default_cache_ttl)
                .build()
            {
                Ok(cache) => Some(cache),
                Err(e) => {
                    telemetry.record(&e);
                    None
                }
            }
        });

        #[cfg(feature = "stateful")]
        let ads_store =
            client_config
                .store_config
                .and_then(|x| match AdsStore::builder(x.db_path).build() {
                    Ok(store) => Some(store),
                    Err(e) => {
                        telemetry.record(&e);
                        None
                    }
                });

        let client = MARSClient::new(environment, http_cache, telemetry.clone());
        telemetry.record(&ClientOperationEvent::New);
        Self {
            client,
            context_id_component,
            context_id_deletion_queue,
            telemetry: telemetry.clone(),
            #[cfg(feature = "stateful")]
            ads_store: Arc::new(Mutex::new(ads_store)),
        }
    }

    pub fn clear_cache(&self) -> Result<(), rusqlite::Error> {
        self.client.clear_cache()
    }

    pub fn get_context_id(&self) -> context_id::ApiResult<String> {
        self.context_id_component.request(DEFAULT_ROTATION_DAYS)
    }

    pub fn record_click(&self, click_url: Url, ohttp: bool) -> Result<(), RecordClickError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut click_url = click_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut click_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }
        self.client
            .record_click(click_url, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::RecordClick);
            })
    }

    pub fn record_impression(
        &self,
        impression_url: Url,
        ohttp: bool,
    ) -> Result<(), RecordImpressionError> {
        // TODO: Re-enable cache invalidation behind a Nimbus experiment.
        // The mobile team has requested this be temporarily disabled.
        // let mut impression_url = impression_url.clone();
        // if let Some(request_hash) = pop_request_hash_from_url(&mut impression_url) {
        //     let _ = self.client.invalidate_cache_by_hash(&request_hash);
        // }

        // TODO: Add count call with _cap_key for impression capping logic
        let impression_url = if let Some((_, _cap_key)) = impression_url
            .query_pairs()
            .find(|(key, _)| key == "cap_key")
        {
            let mut new_url = impression_url.clone();
            new_url
                .query_pairs_mut()
                .clear()
                .extend_pairs(
                    impression_url
                        .query_pairs()
                        .collect::<Vec<_>>()
                        .iter()
                        .filter(|(key, _)| key != "cap_key"),
                )
                .finish();
            new_url
        } else {
            impression_url
        };

        self.client
            .record_impression(impression_url, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry
                    .record(&ClientOperationEvent::RecordImpression);
            })
    }

    pub fn report_ad(
        &self,
        report_url: Url,
        reason: ReportReason,
        ohttp: bool,
    ) -> Result<(), ReportAdError> {
        self.client
            .report_ad(report_url, reason, ohttp)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .inspect(|_| {
                self.telemetry.record(&ClientOperationEvent::ReportAd);
            })
    }

    pub fn request_image_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        flags: AdRequestFlags,
        options: Option<CachePolicy>,
        ohttp: bool,
        blocks: Vec<String>,
    ) -> Result<HashMap<String, AdImage>, RequestAdsError> {
        let response = self
            .request_ads::<AdImage>(ad_placement_requests, flags, options, ohttp, blocks)
            .inspect_err(|e| {
                self.telemetry.record(e);
            })?;
        self.telemetry.record(&ClientOperationEvent::RequestAds);
        Ok(response.take_first())
    }

    pub fn request_spoc_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        flags: AdRequestFlags,
        options: Option<CachePolicy>,
        ohttp: bool,
        blocks: Vec<String>,
    ) -> Result<HashMap<String, Vec<AdSpoc>>, RequestAdsError> {
        let result =
            self.request_ads::<AdSpoc>(ad_placement_requests, flags, options, ohttp, blocks);
        result
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .map(|response| {
                self.telemetry.record(&ClientOperationEvent::RequestAds);
                response.data
            })
    }

    pub fn request_tile_ads(
        &self,
        ad_placement_requests: Vec<AdPlacementRequest>,
        flags: AdRequestFlags,
        options: Option<CachePolicy>,
        ohttp: bool,
        blocks: Vec<String>,
    ) -> Result<HashMap<String, AdTile>, RequestAdsError> {
        let result =
            self.request_ads::<AdTile>(ad_placement_requests, flags, options, ohttp, blocks);
        result
            .inspect_err(|e| {
                self.telemetry.record(e);
            })
            .map(|response| {
                self.telemetry.record(&ClientOperationEvent::RequestAds);
                response.take_first()
            })
    }

    fn request_ads<A>(
        &self,
        placements: Vec<AdPlacementRequest>,
        flags: AdRequestFlags,
        options: Option<CachePolicy>,
        ohttp: bool,
        blocks: Vec<String>,
    ) -> Result<AdResponse<A>, RequestAdsError>
    where
        A: AdResponseValue,
    {
        let context_id = self.get_context_id()?;
        let cache_policy = options.unwrap_or_default();
        let result =
            self.client
                .fetch_ads::<A>(context_id, flags, placements, cache_policy, ohttp, blocks);
        // Flush regardless of the outcome so a failed fetch never strands a
        // retired id; the ad result is returned untouched.
        self.flush_context_id_deletions(ohttp);
        let (mut response, request_hash) = result?;
        response.enrich_callbacks(&request_hash);
        Ok(response)
    }

    /// Sends the deletion request for every context id retired since the
    /// last flush. Best-effort: failures are logged and the ids dropped, and
    /// nothing is sent unless the triggering ad request used OHTTP, so a
    /// retired id is never tied to the client IP in the clear.
    fn flush_context_id_deletions(&self, ohttp: bool) {
        for old_context_id in self.context_id_deletion_queue.take_all() {
            if !ohttp {
                error_support::info!(
                    "Skipping context id deletion request: the ad request did not use OHTTP"
                );
                continue;
            }
            if let Err(e) = self.client.delete_user(&old_context_id, true) {
                error_support::warn!("Context id deletion request failed: {e}");
            }
        }
    }

    pub fn shutdown_references(&self) -> ShutdownReferences<T> {
        ShutdownReferences::new(
            self.telemetry.clone(),
            #[cfg(feature = "stateful")]
            AdsStoreShutdown::new(self.ads_store.clone()),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientOperationEvent {
    New,
    RecordClick,
    RecordImpression,
    ReportAd,
    RequestAds,
}

#[cfg(test)]
mod tests {

    #[cfg(feature = "stateful")]
    use crate::ads_store::builder::AdsStoreBuilder;
    use crate::{
        ffi::telemetry::MozAdsTelemetryWrapper,
        mars::Environment,
        test_utils::{
            get_example_happy_image_response, get_example_happy_spoc_response,
            get_example_happy_uatile_response, make_happy_placement_requests,
        },
    };

    use super::*;

    fn new_with_mars_client(
        client: MARSClient<MozAdsTelemetryWrapper>,
    ) -> AdsClient<MozAdsTelemetryWrapper> {
        new_with_mars_client_at(client, 0).0
    }

    /// Builds a client whose context id was created at `creation_timestamp_s`
    /// (0 = now) and hands back the deletion queue shared with it.
    fn new_with_mars_client_at(
        client: MARSClient<MozAdsTelemetryWrapper>,
        creation_timestamp_s: i64,
    ) -> (AdsClient<MozAdsTelemetryWrapper>, ContextIdDeletionQueue) {
        let telemetry = client.get_telemetry();
        let context_id_deletion_queue = ContextIdDeletionQueue::default();
        let ads_client = AdsClient {
            client,
            context_id_component: ContextIDComponent::new(
                &Uuid::new_v4().to_string(),
                creation_timestamp_s,
                DISABLE_CONTEXT_ID_COMPONENT_DELETION,
                Box::new(context_id_deletion_queue.clone()),
            ),
            context_id_deletion_queue: context_id_deletion_queue.clone(),
            telemetry,
            #[cfg(feature = "stateful")]
            ads_store: Arc::new(Mutex::new(Some(
                AdsStoreBuilder::new("test_store.db")
                    .build()
                    .expect("Simplest AdsStoreBuilder should be constructable"),
            ))),
        };
        (ads_client, context_id_deletion_queue)
    }

    fn thirty_days_ago_s() -> i64 {
        (chrono::Utc::now() - chrono::Duration::days(30)).timestamp()
    }

    #[test]
    fn test_rotation_with_plaintext_request_skips_delete() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_uatile_response();
        let ads_mock = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .expect(1)
            .create();
        let delete_mock = mockito::mock("DELETE", "/delete_user").expect(0).create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let (ads_client, queue) = new_with_mars_client_at(mars_client, thirty_days_ago_s());

        // The id is 30 days old, so the first request rotates it and the old
        // id lands in the queue...
        ads_client.get_context_id().unwrap();
        assert!(!queue.is_empty());

        // ...and a plaintext ad request flushes the queue WITHOUT sending
        // the deletion in the clear.
        let result = ads_client.request_tile_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            false,
            Default::default(),
        );
        assert!(result.is_ok());
        ads_mock.assert();
        delete_mock.assert();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_rotation_flush_runs_even_when_fetch_fails() {
        viaduct_dev::init_backend_dev();

        // OHTTP requested but no channel configured: the ad fetch fails, the
        // queue is still drained, and nothing is sent in the clear.
        let delete_mock = mockito::mock("DELETE", "/delete_user").expect(0).create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let (ads_client, queue) = new_with_mars_client_at(mars_client, thirty_days_ago_s());
        ads_client.get_context_id().unwrap();
        assert!(!queue.is_empty());

        let result = ads_client.request_tile_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            true,
            Default::default(),
        );
        assert!(result.is_err());
        delete_mock.assert();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_get_context_id() {
        let config = AdsClientConfig {
            cache_config: None,
            environment: Environment::Test,
            telemetry: MozAdsTelemetryWrapper::noop(),
            #[cfg(feature = "stateful")]
            store_config: None,
        };
        let client = AdsClient::new(config);
        let context_id = client.get_context_id().unwrap();
        assert!(!context_id.is_empty());
    }

    #[test]
    fn test_request_image_ads_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_image_response();
        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_image_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            false,
            Default::default(),
        );
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_request_spocs_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_spoc_response();
        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_spoc_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            false,
            Default::default(),
        );
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_request_tiles_happy() {
        viaduct_dev::init_backend_dev();

        let expected_response = get_example_happy_uatile_response();
        let m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let result = ads_client.request_tile_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            false,
            Default::default(),
        );
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_context_id_is_sent_to_mars() {
        viaduct_dev::init_backend_dev();

        let config = AdsClientConfig {
            cache_config: None,
            environment: Environment::Test,
            telemetry: MozAdsTelemetryWrapper::noop(),
            #[cfg(feature = "stateful")]
            store_config: None,
        };
        let client = AdsClient::new(config);

        // The client generates its own context id, so read it back first and
        // assert that exactly that value reaches the wire.
        let context_id = client.get_context_id().unwrap();
        assert!(Uuid::parse_str(&context_id).is_ok());

        let expected_response = get_example_happy_image_response();
        let m = mockito::mock("POST", "/ads")
            .match_body(mockito::Matcher::PartialJsonString(format!(
                r#"{{"context_id":"{context_id}"}}"#
            )))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&expected_response.data).unwrap())
            .create();

        let result = client.request_image_ads(
            make_happy_placement_requests(),
            AdRequestFlags::default(),
            None,
            false,
            Default::default(),
        );
        assert!(result.is_ok());
        m.assert();
    }

    #[test]
    fn test_record_impression_removes_cap_key() {
        viaduct_dev::init_backend_dev();
        let mars_client = MARSClient::new(Environment::Test, None, MozAdsTelemetryWrapper::noop());
        let ads_client = new_with_mars_client(mars_client);

        let base_url = mockito::server_url();
        let path_and_query = "/impression?kept=example";
        let callback_url = Url::parse(&format!("{}{}", base_url, path_and_query)).unwrap();

        let mock = mockito::mock("GET", path_and_query)
            .with_status(200)
            .create();

        ads_client.record_impression(callback_url, false).unwrap();

        mock.assert();

        let callback_url_with_cap_key =
            Url::parse(&format!("{}{}&cap_key=test", base_url, path_and_query)).unwrap();
        ads_client
            .record_impression(callback_url_with_cap_key, false)
            .unwrap();

        mock.expect(2).assert();
    }

    #[test]
    #[ignore = "Cache invalidation temporarily disabled - will be re-enabled behind Nimbus experiment"]
    fn test_record_click_invalidates_cache() {
        viaduct_dev::init_backend_dev();
        let cache = HttpCache::builder("test_record_click_invalidates_cache")
            .build()
            .unwrap();
        let mars_client = MARSClient::new(
            Environment::Test,
            Some(cache),
            MozAdsTelemetryWrapper::noop(),
        );
        let ads_client = new_with_mars_client(mars_client);

        let response = get_example_happy_image_response();

        let m1 = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&response.data).unwrap())
            .expect(2) // we expect 2 requests to the server, one for the initial ad request and one after for the cache invalidation request
            .create();

        let response = ads_client
            .request_image_ads(
                make_happy_placement_requests(),
                AdRequestFlags::default(),
                None,
                false,
                Default::default(),
            )
            .unwrap();
        let callback_url = response.values().next().unwrap().callbacks.click.clone();

        let m2 = mockito::mock("GET", callback_url.path())
            .with_status(200)
            .create();

        ads_client
            .request_image_ads(
                make_happy_placement_requests(),
                AdRequestFlags::default(),
                None,
                false,
                Default::default(),
            )
            .unwrap();

        ads_client.record_click(callback_url, false).unwrap();

        ads_client
            .request_ads::<AdImage>(
                make_happy_placement_requests(),
                AdRequestFlags::default(),
                Some(CachePolicy::default()),
                false,
                Default::default(),
            )
            .unwrap();

        m1.assert();
        m2.assert();
    }
}
