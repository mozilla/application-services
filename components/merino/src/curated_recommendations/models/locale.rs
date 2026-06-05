/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

/// Locales supported by Merino curated recommendations.
///
/// Each variant maps to a BCP 47 locale string (e.g. `"en-US"`, `"fr"`) used when
/// requesting recommendations from the Merino API.
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
            "fr".to_string(),
            "fr-FR".to_string(),
            "es".to_string(),
            "es-ES".to_string(),
            "it".to_string(),
            "it-IT".to_string(),
            "en".to_string(),
            "en-CA".to_string(),
            "en-GB".to_string(),
            "en-US".to_string(),
            "de".to_string(),
            "de-DE".to_string(),
            "de-AT".to_string(),
            "de-CH".to_string(),
        ]
    }

    /// Parses a locale string (e.g. `"en-US"`) into a `CuratedRecommendationLocale`
    /// enum variant.
    ///
    /// Returns `None` if the string does not match a known variant.
    pub fn from_locale_string(locale: String) -> Option<CuratedRecommendationLocale> {
        match locale.as_str() {
            "fr" => Some(CuratedRecommendationLocale::Fr),
            "fr-FR" => Some(CuratedRecommendationLocale::FrFr),
            "es" => Some(CuratedRecommendationLocale::Es),
            "es-ES" => Some(CuratedRecommendationLocale::EsEs),
            "it" => Some(CuratedRecommendationLocale::It),
            "it-IT" => Some(CuratedRecommendationLocale::ItIt),
            "en" => Some(CuratedRecommendationLocale::En),
            "en-CA" => Some(CuratedRecommendationLocale::EnCa),
            "en-GB" => Some(CuratedRecommendationLocale::EnGb),
            "en-US" => Some(CuratedRecommendationLocale::EnUs),
            "de" => Some(CuratedRecommendationLocale::De),
            "de-DE" => Some(CuratedRecommendationLocale::DeDe),
            "de-AT" => Some(CuratedRecommendationLocale::DeAt),
            "de-CH" => Some(CuratedRecommendationLocale::DeCh),
            _ => None,
        }
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

    #[test]
    fn test_from_locale_string_empty_string() {
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("".into()),
            None
        );
    }

    #[test]
    fn test_from_locale_string_is_case_sensitive() {
        // Locales are case-sensitive BCP 47 strings
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("EN-US".into()),
            None
        );
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("en-us".into()),
            None
        );
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("Fr".into()),
            None
        );
        // Only the exact canonical form should work
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("en-US".into()),
            Some(CuratedRecommendationLocale::EnUs)
        );
        assert_eq!(
            CuratedRecommendationLocale::from_locale_string("fr".into()),
            Some(CuratedRecommendationLocale::Fr)
        );
    }
}
