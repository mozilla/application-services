use std::collections::HashMap;

use error_support::handle_error;

use crate::{
    ads_store::StorableAd,
    client::error::{ComponentError, RequestAdsError},
    http_cache::CachePolicy,
    mars::ad_request::AdPlacementRequest,
    AdsClientApiResult, MozAdsClientInner, MozAdsPlacementRequest, MozAdsPlacementRequestWithCount,
};

// Command dispatch enum for passing different instructions to the background worker thread.
// `RequestImageAds`, `RequestSpocAds`, `RequestTileAds` are prefetch mechanisms that query and load data into the local cache.
pub enum DispatchCommand {
    RequestImageAds {
        image_ad_requests: Vec<MozAdsPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
        blocks: Vec<String>,
    },
    RequestSpocAds {
        spoc_ad_requests: Vec<MozAdsPlacementRequestWithCount>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
        blocks: Vec<String>,
    },
    RequestTileAds {
        tile_ad_requests: Vec<MozAdsPlacementRequest>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
        blocks: Vec<String>,
    },
}

impl DispatchCommand {
    // Runs a dispatched command synchronously in it's thread.
    // The dispatched command calls the corresponding `AdsClient` synchronous method, meaning that behavior between the two is shared.
    #[handle_error(ComponentError)]
    pub fn run_command(self, ads_client_inner: &MozAdsClientInner) -> AdsClientApiResult<()> {
        match self {
            DispatchCommand::RequestImageAds {
                image_ad_requests,
                cache_policy,
                flags,
                ohttp,
                blocks,
            } => {
                let mut inner = ads_client_inner.lock();
                if !image_ad_requests.is_empty() {
                    let image_ad_requests: Vec<AdPlacementRequest> =
                        image_ad_requests.iter().map(|r| r.into()).collect();
                    let image_response = inner
                        .request_image_ads(
                            image_ad_requests,
                            flags,
                            Some(cache_policy),
                            ohttp,
                            blocks,
                        )
                        .map_err(ComponentError::RequestAds)?;
                    inner
                        .cache_ads(
                            image_response
                                .into_iter()
                                .map(|(k, v)| (k.into(), StorableAd::Image(v)))
                                .collect(),
                        )
                        .map_err(RequestAdsError::from)?;
                }
                Ok(())
            }
            DispatchCommand::RequestSpocAds {
                spoc_ad_requests,
                cache_policy,
                flags,
                ohttp,
                blocks,
            } => {
                let mut inner = ads_client_inner.lock();
                if !spoc_ad_requests.is_empty() {
                    let spoc_ad_requests: Vec<AdPlacementRequest> =
                        spoc_ad_requests.iter().map(|r| r.into()).collect();
                    let spoc_response = inner
                        .request_spoc_ads(
                            spoc_ad_requests,
                            flags,
                            Some(cache_policy),
                            ohttp,
                            blocks,
                        )
                        .map_err(ComponentError::RequestAds)?;
                    inner
                        .cache_ads(
                            spoc_response
                                .into_iter()
                                .map(|(k, v)| (k.into(), StorableAd::Spoc(v)))
                                .collect(),
                        )
                        .map_err(RequestAdsError::from)?;
                }
                Ok(())
            }
            DispatchCommand::RequestTileAds {
                tile_ad_requests,
                cache_policy,
                flags,
                ohttp,
                blocks,
            } => {
                let mut inner = ads_client_inner.lock();
                if !tile_ad_requests.is_empty() {
                    let tile_ad_requests: Vec<AdPlacementRequest> =
                        tile_ad_requests.iter().map(|r| r.into()).collect();
                    let tile_response = inner
                        .request_tile_ads(
                            tile_ad_requests,
                            flags,
                            Some(cache_policy),
                            ohttp,
                            blocks,
                        )
                        .map_err(ComponentError::RequestAds)?;
                    inner
                        .cache_ads(
                            tile_response
                                .into_iter()
                                .map(|(k, v)| (k.into(), StorableAd::Tile(v)))
                                .collect(),
                        )
                        .map_err(RequestAdsError::from)?;
                }
                Ok(())
            }
        }
    }
}
