/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

/// Defines the `CuratedRecommendationLocale` enum along with `all_locales()` and
/// `from_locale_string()` methods, ensuring the variant list is specified exactly once.
macro_rules! define_locales {
    ( $( $variant:ident => $str:literal ),+ $(,)? ) => {
        /// Locales supported by Merino curated recommendations.
        ///
        /// Each variant maps to a BCP 47 locale string (e.g. `"en-US"`, `"fr"`) used when
        /// requesting recommendations from the Merino API.
        #[derive(Debug, Serialize, PartialEq, Deserialize, uniffi::Enum)]
        pub enum CuratedRecommendationLocale {
            $(
                #[serde(rename = $str)]
                $variant,
            )+
        }

        impl CuratedRecommendationLocale {
            /// Returns all supported locale strings (e.g. `"en-US"`, `"fr-FR"`).
            ///
            /// These strings are the canonical serialized values of the enum variants.
            pub fn all_locales() -> Vec<String> {
                vec![ $( $str.to_string(), )+ ]
            }

            /// Parses a locale string (e.g. `"en-US"`) into a `CuratedRecommendationLocale`
            /// enum variant.
            ///
            /// Returns `None` if the string does not match a known variant.
            pub fn from_locale_string(locale: String) -> Option<CuratedRecommendationLocale> {
                match locale.as_str() {
                    $( $str => Some(CuratedRecommendationLocale::$variant), )+
                    _ => None,
                }
            }
        }
    };
}

define_locales! {
    Fr    => "fr",
    FrFr  => "fr-FR",
    Es    => "es",
    EsEs  => "es-ES",
    It    => "it",
    ItIt  => "it-IT",
    En    => "en",
    EnCa  => "en-CA",
    EnGb  => "en-GB",
    EnUs  => "en-US",
    De    => "de",
    DeDe  => "de-DE",
    DeAt  => "de-AT",
    DeCh  => "de-CH",
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
