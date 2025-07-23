/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */
use serde::{Deserialize, Serialize};

// Configuration options for initializing a `CuratedRecommendationsClient`
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsConfig {
    pub base_host: Option<String>,
    pub user_agent_header: String,
}

// Locales supported by Merino Curated Recommendations
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
pub enum CuratedRecommendationLocale {
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "fr-FR")]
    FrFr,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "es-ES")]
    EsEs,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "it-IT")]
    ItIt,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "en-CA")]
    EnCa,
    #[serde(rename = "en-GB")]
    EnGb,
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "de-DE")]
    DeDe,
    #[serde(rename = "de-AT")]
    DeAt,
    #[serde(rename = "de-CH")]
    DeCh,
}

impl CuratedRecommendationLocale {
    /// Returns all supported locale strings (e.g. `"en-US"`, `"fr-FR"`).
    ///
    /// These strings are the canonical serialized values of the enum variants.
    pub fn all_locales() -> Vec<String> {
        vec![
            CuratedRecommendationLocale::Fr,
            CuratedRecommendationLocale::FrFr,
            CuratedRecommendationLocale::Es,
            CuratedRecommendationLocale::EsEs,
            CuratedRecommendationLocale::It,
            CuratedRecommendationLocale::ItIt,
            CuratedRecommendationLocale::En,
            CuratedRecommendationLocale::EnCa,
            CuratedRecommendationLocale::EnGb,
            CuratedRecommendationLocale::EnUs,
            CuratedRecommendationLocale::De,
            CuratedRecommendationLocale::DeDe,
            CuratedRecommendationLocale::DeAt,
            CuratedRecommendationLocale::DeCh,
        ]
        .into_iter()
        .map(|l| {
            serde_json::to_string(&l)
                .unwrap()
                .trim_matches('"')
                .to_string()
        })
        .collect()
    }

    /// Parses a locale string (e.g. `"en-US"`) into a `CuratedRecommendationLocale` enum variant.
    ///
    /// Returns `None` if the string does not match a known variant.
    pub fn from_locale_string(locale: String) -> Option<CuratedRecommendationLocale> {
        serde_json::from_str(&format!("\"{}\"", locale)).ok()
    }
}

// Configuration settings for a Section
#[derive(Debug, Serialize, Deserialize, PartialEq, uniffi::Record)]
pub struct SectionSettings {
    #[serde(rename = "sectionId")]
    pub section_id: String,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

// Information required to request curated recommendations
#[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Record)]
pub struct CuratedRecommendationsRequest {
    pub locale: CuratedRecommendationLocale,
    #[uniffi(default = None)]
    pub region: Option<String>,
    #[uniffi(default = Some(100))]
    pub count: Option<i32>,
    #[uniffi(default = None)]
    pub topics: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub feeds: Option<Vec<String>>,
    #[uniffi(default = None)]
    pub sections: Option<Vec<SectionSettings>>,
    #[serde(rename = "experimentName")]
    #[uniffi(default = None)]
    pub experiment_name: Option<String>,
    #[serde(rename = "experimentBranch")]
    #[uniffi(default = None)]
    pub experiment_branch: Option<String>,
    #[serde(rename = "enableInterestPicker", default)]
    #[uniffi(default = false)]
    pub enable_interest_picker: bool,
}

// Response schema for a list of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsResponse {
    #[serde(rename = "recommendedAt")]
    pub recommended_at: i64,
    pub data: Vec<RecommendationDataItem>,
    #[uniffi(default = None)]
    pub feeds: Option<Feeds>,
    #[serde(rename = "interestPicker")]
    #[uniffi(default = None)]
    pub interest_picker: Option<InterestPicker>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
// Specifies the display order (receivedFeedRank) and a list of sections (referenced by sectionId) for interest bubbles.
pub struct InterestPicker {
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    pub title: String,
    pub subtitle: String,
    pub sections: Vec<InterestPickerSection>,
}

#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct InterestPickerSection {
    #[serde(rename = "sectionId")]
    pub section_id: String,
}

// Multiple lists of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Feeds {
    #[uniffi(default = None)]
    pub need_to_know: Option<CuratedRecommendationsBucket>,
    #[uniffi(default = None)]
    pub fakespot: Option<FakespotFeed>,
    #[uniffi(default = None)]
    pub top_stories_section: Option<FeedSection>,
    #[uniffi(default = None)]
    pub business: Option<FeedSection>,
    #[uniffi(default = None)]
    pub career: Option<FeedSection>,
    #[uniffi(default = None)]
    pub arts: Option<FeedSection>,
    #[uniffi(default = None)]
    pub food: Option<FeedSection>,
    #[uniffi(default = None)]
    pub health: Option<FeedSection>,
    #[uniffi(default = None)]
    pub home: Option<FeedSection>,
    #[uniffi(default = None)]
    pub finance: Option<FeedSection>,
    #[uniffi(default = None)]
    pub government: Option<FeedSection>,
    #[uniffi(default = None)]
    pub sports: Option<FeedSection>,
    #[uniffi(default = None)]
    pub tech: Option<FeedSection>,
    #[uniffi(default = None)]
    pub travel: Option<FeedSection>,
    #[uniffi(default = None)]
    pub education: Option<FeedSection>,
    #[uniffi(default = None)]
    pub hobbies: Option<FeedSection>,
    #[serde(rename = "society-parenting")]
    #[uniffi(default = None)]
    pub society_parenting: Option<FeedSection>,
    #[serde(rename = "education-science")]
    #[uniffi(default = None)]
    pub education_science: Option<FeedSection>,
    #[uniffi(default = None)]
    pub society: Option<FeedSection>,
}

// Curated Recommendation Information
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct RecommendationDataItem {
    #[serde(rename = "corpusItemId")]
    pub corpus_item_id: String,
    #[serde(rename = "scheduledCorpusItemId")]
    pub scheduled_corpus_item_id: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    #[uniffi(default = None)]
    pub topic: Option<String>,
    pub publisher: String,
    #[serde(rename = "isTimeSensitive")]
    pub is_time_sensitive: bool,
    #[serde(rename = "imageUrl")]
    pub image_url: String,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(rename = "tileId")]
    pub tile_id: i64,
    #[serde(rename = "receivedRank")]
    pub received_rank: i64,
}

// Ranked list of curated recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct CuratedRecommendationsBucket {
    pub recommendations: Vec<RecommendationDataItem>,
    #[uniffi(default = None)]
    pub title: Option<String>,
}

// Fakespot product recommendations
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotFeed {
    pub products: Vec<FakespotProduct>,
    #[serde(rename = "defaultCategoryName")]
    pub default_category_name: String,
    #[serde(rename = "headerCopy")]
    pub header_copy: String,
    #[serde(rename = "footerCopy")]
    pub footer_copy: String,
    pub cta: FakespotCta,
}

// Fakespot product details
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotProduct {
    id: String,
    title: String,
    category: String,
    #[serde(rename = "imageUrl")]
    image_url: String,
    url: String,
}

// Fakespot CTA
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FakespotCta {
    #[serde(rename = "ctaCopy")]
    pub cta_copy: String,
    pub url: String,
}

// Ranked list of curated recommendations with responsive layout configs
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct FeedSection {
    #[serde(rename = "receivedFeedRank")]
    pub received_feed_rank: i32,
    pub recommendations: Vec<RecommendationDataItem>,
    pub title: String,
    #[uniffi(default = None)]
    pub subtitle: Option<String>,
    pub layout: Layout,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
    #[serde(rename = "isBlocked")]
    pub is_blocked: bool,
}

// Representation of a responsive layout configuration with multiple column layouts
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Layout {
    pub name: String,
    #[serde(rename = "responsiveLayouts")]
    pub responsive_layouts: Vec<ResponsiveLayout>,
}

// Layout configurations within a column
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct ResponsiveLayout {
    #[serde(rename = "columnCount")]
    pub column_count: i32,
    pub tiles: Vec<Tile>,
}
// Properties for a single tile in a responsive layout
#[derive(Debug, Deserialize, PartialEq, uniffi::Record, Serialize)]
pub struct Tile {
    pub size: String,
    pub position: i32,
    #[serde(rename = "hasAd")]
    pub has_ad: bool,
    #[serde(rename = "hasExcerpt")]
    pub has_excerpt: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_locale_string_valid_cases() {
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("en-US".into()),
            Some(CuratedRecommendationLocale::EnUs)
        );
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("fr".into()),
            Some(CuratedRecommendationLocale::Fr)
        );
    }

    #[test]
    fn test_from_locale_string_invalid_cases() {
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("en_US".into()),
            None
        );
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("zz-ZZ".into()),
            None
        );
    }

    #[test]
    fn test_all_locales_contains_expected_values() {
        let locales = CuratedRecommendationLocale::all_locales();
        assert!(locales.contains(&"en-US".to_string()));
        assert!(locales.contains(&"de-CH".to_string()));
        assert!(locales.contains(&"fr".to_string()));
    }

    #[test]
    fn test_all_locales_round_trip() {
        for locale_str in CuratedRecommendationLocale::all_locales() {
            let parsed = CuratedRecommendationLocale::from_locale_string(locale_str.clone());
            assert!(parsed.is_some(), "Failed to parse locale: {}", locale_str);

            let reserialized = serde_json::to_string(&parsed.unwrap()).unwrap();
            let clean = reserialized.trim_matches('"');
            assert_eq!(
                clean, locale_str,
                "Round-trip mismatch: {} => {}",
                locale_str, clean
            );
        }
    }
}
