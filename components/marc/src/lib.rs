/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use models::MozAd;
use serde::{Deserialize, Serialize};

mod error;
mod mars;
mod models;

uniffi::setup_scaffolding!("MARC");

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
