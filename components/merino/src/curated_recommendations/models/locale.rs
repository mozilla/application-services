/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

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
