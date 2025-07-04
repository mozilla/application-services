/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use error::{ApiResult, Error, Result};
use error_support::handle_error;
use mars::{DefaultMARSClient, MARSClient};
use models::{AdRequest, AdResponse, MozAd};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod error;
mod mars;
mod models;

uniffi::setup_scaffolding!("MARC");

/// Top-level API for the marc component
#[derive(uniffi::Object)]
pub struct MozAdsComponent {
    inner: Mutex<MozAdsComponentInner>,
}

#[uniffi::export]
impl MozAdsComponent {
    #[uniffi::constructor]
    fn new() -> Self {
        Self {
            inner: Mutex::new(MozAdsComponentInner::new()),
        }
    }

    #[handle_error(Error)]
    pub fn request_ads(
        &self,
        moz_ad_configs: Vec<MozAdsPlacementConfig>,
    ) -> ApiResult<HashMap<String, MozAdsPlacement>> {
        let mut inner = self.inner.lock();
        let placements = inner.request_ads(&moz_ad_configs)?;
        Ok(placements)
    }

    #[handle_error(Error)]
    pub fn record_impression(&self, placement: MozAdsPlacement) -> ApiResult<()> {
        let inner = self.inner.lock();
        inner.record_impression(&placement)?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn record_click(&self, placement: MozAdsPlacement) -> ApiResult<()> {
        let inner = self.inner.lock();
        inner.record_click(&placement)?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn record_report_ad(&self, placement: MozAdsPlacement) -> ApiResult<()> {
        let inner = self.inner.lock();
        inner.record_report_ad(&placement)?;
        Ok(())
    }

    #[handle_error(Error)]
    pub fn cycle_context_id(&self) -> ApiResult<String> {
        let mut inner = self.inner.lock();
        let previous_context_id = inner.cycle_context_id();
        Ok(previous_context_id)
    }

    #[handle_error(Error)]
    pub fn clear_cache(&self) -> ApiResult<()> {
        let mut inner = self.inner.lock();
        inner.clear_cache();
        Ok(())
    }
}

pub struct MozAdsComponentInner {
    ads_cache: HashMap<String, MozAdsPlacement>, //TODO: implement caching
    client: Box<dyn MARSClient>,
}

impl MozAdsComponentInner {
    fn new() -> Self {
        let context_id = Uuid::new_v4().to_string();
        let client = Box::new(DefaultMARSClient::new(context_id));
        let ads_cache = HashMap::new(); //TODO: HashMap is a placeholder.
        Self { ads_cache, client }
    }

    fn clear_cache(&mut self) {
        self.ads_cache.clear();
    }

    fn request_ads(
        &mut self,
        moz_ad_configs: &Vec<MozAdsPlacementConfig>,
    ) -> Result<HashMap<String, MozAdsPlacement>> {
        let ad_request = self.build_request_from_placement_configs(moz_ad_configs);
        let response = self.client.fetch_ads(&ad_request)?;
        let placements = self.build_placements(moz_ad_configs, response)?;
        Ok(placements)
    }

    fn record_impression(&self, placement: &MozAdsPlacement) -> Result<()> {
        let impression_callback = placement
            .content
            .callbacks
            .as_ref()
            .and_then(|callbacks| callbacks.impression.as_ref());

        self.client.record_impression(impression_callback)?;
        Ok(())
    }

    fn record_click(&self, placement: &MozAdsPlacement) -> Result<()> {
        let click_callback = placement
            .content
            .callbacks
            .as_ref()
            .and_then(|callbacks| callbacks.click.as_ref());

        self.client.record_click(click_callback)?;
        Ok(())
    }

    fn record_report_ad(&self, placement: &MozAdsPlacement) -> Result<()> {
        let report_ad_callback = placement
            .content
            .callbacks
            .as_ref()
            .and_then(|callbacks| callbacks.report.as_ref());

        self.client.record_report_ad(report_ad_callback)?;
        Ok(())
    }

    fn cycle_context_id(&mut self) -> String {
        self.client.cycle_context_id()
    }

    fn build_request_from_placement_configs(
        &self,
        moz_ad_configs: &Vec<MozAdsPlacementConfig>,
    ) -> AdRequest {
        let context_id = self.client.get_context_id().to_string();
        let mut request = AdRequest {
            placements: vec![],
            context_id,
        };

        for config in moz_ad_configs {
            request.placements.push(models::AdPlacementRequest {
                placement: config.placement_id.clone(),
                count: 1, // Placement_id should be treated as unique, so count is always 1
                content: None,
            });
        }

        request
    }

    //TODO: This could probably be refactored to be cleaner
    fn build_placements(
        &self,
        placement_configs: &Vec<MozAdsPlacementConfig>,
        mut mars_response: AdResponse,
    ) -> Result<HashMap<String, MozAdsPlacement>> {
        let mut moz_ad_placements: HashMap<String, MozAdsPlacement> = HashMap::new();

        for config in placement_configs {
            let placement_content = mars_response.data.get_mut(&config.placement_id);

            match placement_content {
                Some(v) => {
                    let ad_content = v.pop();
                    match ad_content {
                        Some(c) => {
                            let is_updated = moz_ad_placements.insert(
                                config.placement_id.clone(),
                                MozAdsPlacement {
                                    content: c,
                                    placement_config: config.clone(),
                                },
                            );
                            if let Some(v) = is_updated {
                                return Err(Error::DuplicatePlacementId {
                                    placement_id: v.placement_config.placement_id,
                                });
                            }
                        }
                        None => continue,
                    }
                }
                None => continue,
            }
        }

        Ok(moz_ad_placements)
    }
}

#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
pub enum IABAdUnitFormat {
    Billboard,
    SmartphoneBanner300,
    SmartphoneBanner320,
    Leaderboard,
    SuperLeaderboardPushdown,
    Portrait,
    Skyscraper,
    MediumRectangle,
    TwentyBySixty,
    MobilePhoneInterstitial640,
    MobilePhoneInterstitial750,
    MobilePhoneInterstitial1080,
    FeaturePhoneSmallBanner,
    FeaturePhoneMediumBanner,
    FeaturePhoneLargeBanner,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
pub enum IABContentTaxonomy {
    #[serde(rename = "IAB-1.0")]
    IAB1_0,

    #[serde(rename = "IAB-2.0")]
    IAB2_0,

    #[serde(rename = "IAB-2.1")]
    IAB2_1,

    #[serde(rename = "IAB-2.2")]
    IAB2_2,

    #[serde(rename = "IAB-3.0")]
    IAB3_0,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IABContent {
    pub taxonomy: IABContentTaxonomy,
    pub category_ids: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MozAdsSize {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MozAdsPlacementConfig {
    pub placement_id: String,
    pub fixed_size: Option<MozAdsSize>,
    pub iab_content: Option<IABContent>,
}

#[derive(Debug, uniffi::Record)]
pub struct MozAdsPlacement {
    pub placement_config: MozAdsPlacementConfig,
    pub content: MozAd,
}
