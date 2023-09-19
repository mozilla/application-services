/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use chrono::Local;

/// The template parameter for a timestamp in a "raw" sponsored suggestion URL.
const TIMESTAMP_TEMPLATE: &str = "%YYYYMMDDHH%";

/// The length, in bytes, of a timestamp in a "cooked" sponsored suggestion URL.
///
/// Cooked timestamps don't include the leading or trailing `%`, so this is
/// 2 bytes shorter than [`TIMESTAMP_TEMPLATE`].
const TIMESTAMP_LENGTH: usize = 10;

/// A suggestion from the database to show in the address bar.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Suggestion {
    Amp {
        title: String,
        url: String,
        raw_url: String,
        icon: Option<Vec<u8>>,
        full_keyword: String,
        block_id: i64,
        advertiser: String,
        iab_category: String,
        impression_url: String,
        click_url: String,
        raw_click_url: String,
    },
    Wikipedia {
        title: String,
        url: String,
        icon: Option<Vec<u8>>,
        full_keyword: String,
    },
}

impl Suggestion {
    /// Returns `true` if the suggestion is sponsored.
    pub(crate) fn is_sponsored(&self) -> bool {
        matches!(self, Self::Amp { .. })
    }
}

/// Replaces all template parameters in a "raw" sponsored suggestion URL,
/// producing a "cooked" URL with real values.
pub(crate) fn cook_raw_suggestion_url(raw_url: &str) -> String {
    let replacement = Local::now().format("%Y%m%d%H").to_string();
    debug_assert!(replacement.len() == TIMESTAMP_LENGTH);
    raw_url.replace(TIMESTAMP_TEMPLATE, &replacement)
}

/// Determines whether a "raw" sponsored suggestion URL is equivalent to a
/// "cooked" URL. The two URLs are equivalent if they are identical except for
/// their replaced template parameters, which can be different.
pub fn raw_suggestion_url_matches(raw_url: &str, cooked_url: &str) -> bool {
    let mut last_raw_url_index = 0;

    // The running difference between indices in the raw URL and the
    // corresponding indices in the cooked URL.
    let mut cooked_url_diff = 0;

    // Ensure that the segments between the timestamps are the same.
    for (raw_url_index, _) in raw_url.match_indices(TIMESTAMP_TEMPLATE) {
        let raw_url_segment = &raw_url[last_raw_url_index..raw_url_index];
        let Some(cooked_url_segment) =
            cooked_url.get(last_raw_url_index - cooked_url_diff..raw_url_index - cooked_url_diff)
        else {
            // The corresponding indices in the cooked URL are out-of-bounds,
            // so the URLs can't match.
            return false;
        };
        if raw_url_segment != cooked_url_segment {
            // The corresponding segments between the last timestamp and this
            // timestamp are different, so the URLs can't match.
            return false;
        }
        last_raw_url_index = raw_url_index + TIMESTAMP_TEMPLATE.len();
        cooked_url_diff += TIMESTAMP_TEMPLATE.len() - TIMESTAMP_LENGTH;
    }

    // Ensure that the last corresponding segments, after the last timestamp,
    // are the same.
    let last_raw_url_segment = &raw_url[last_raw_url_index..];
    match cooked_url.get(last_raw_url_index - cooked_url_diff..) {
        Some(last_cooked_url_segment) => last_raw_url_segment == last_cooked_url_segment,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cook_url_with_template_parameters() {
        let raw_url_with_one_timestamp = "https://example.com?a=%YYYYMMDDHH%";
        let cooked_url_with_one_timestamp = cook_raw_suggestion_url(raw_url_with_one_timestamp);
        assert_eq!(
            cooked_url_with_one_timestamp.len(),
            raw_url_with_one_timestamp.len() - 2
        );
        assert_ne!(raw_url_with_one_timestamp, cooked_url_with_one_timestamp);

        let raw_url_with_trailing_segment = "https://example.com?a=%YYYYMMDDHH%&b=c";
        let cooked_url_with_trailing_segment =
            cook_raw_suggestion_url(raw_url_with_trailing_segment);
        assert_eq!(
            cooked_url_with_trailing_segment.len(),
            raw_url_with_trailing_segment.len() - 2
        );
        assert_ne!(
            raw_url_with_trailing_segment,
            cooked_url_with_trailing_segment
        );

        let raw_url_with_two_timestamps = "https://example.com?a=%YYYYMMDDHH%&b=%YYYYMMDDHH%";
        let cooked_url_with_two_timestamps = cook_raw_suggestion_url(raw_url_with_two_timestamps);
        assert_eq!(
            cooked_url_with_two_timestamps.len(),
            raw_url_with_two_timestamps.len() - 4
        );
        assert_ne!(raw_url_with_two_timestamps, cooked_url_with_two_timestamps);
    }

    #[test]
    fn cook_url_without_template_parameters() {
        assert_eq!(
            cook_raw_suggestion_url("http://example.com/123"),
            "http://example.com/123"
        );
    }

    #[test]
    fn url_with_template_parameters_matches() {
        let raw_url_with_one_timestamp = "https://example.com?a=%YYYYMMDDHH%";
        let raw_url_with_trailing_segment = "https://example.com?a=%YYYYMMDDHH%&b=c";
        let raw_url_with_two_timestamps = "https://example.com?a=%YYYYMMDDHH%&b=%YYYYMMDDHH%";

        // Equivalent, except for their replaced template parameters.
        assert!(raw_suggestion_url_matches(
            raw_url_with_one_timestamp,
            "https://example.com?a=0000000000"
        ));
        assert!(raw_suggestion_url_matches(
            raw_url_with_trailing_segment,
            "https://example.com?a=1111111111&b=c"
        ));
        assert!(raw_suggestion_url_matches(
            raw_url_with_two_timestamps,
            "https://example.com?a=2222222222&b=3333333333"
        ));

        // Different lengths.
        assert!(!raw_suggestion_url_matches(
            raw_url_with_one_timestamp,
            "https://example.com?a=1234567890&c=d"
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_one_timestamp,
            "https://example.com?a=123456789"
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_trailing_segment,
            "https://example.com?a=0987654321"
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_trailing_segment,
            "https://example.com?a=0987654321&b=c&d=e"
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_two_timestamps,
            "https://example.com?a=456123789"
        ));

        // Different query parameter names.
        assert!(!raw_suggestion_url_matches(
            raw_url_with_one_timestamp,         // `a`.
            "https://example.com?b=4444444444"  // `b`.
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_trailing_segment,          // `a&b`.
            "https://example.com?a=5555555555&c=c"  // `a&c`.
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url_with_two_timestamps,                     // `a&b`.
            "https://example.com?a=6666666666&c=7777777777"  // `a&c`.
        ));
    }

    #[test]
    fn url_without_template_parameters_matches() {
        let raw_url = "http://example.com/123";

        assert!(raw_suggestion_url_matches(
            raw_url,
            "http://example.com/123"
        ));
        assert!(!raw_suggestion_url_matches(raw_url, "http://example.com"));
        assert!(!raw_suggestion_url_matches(
            raw_url,
            "http://example.com/456"
        ));
        assert!(!raw_suggestion_url_matches(
            raw_url,
            "http://example.com/123456"
        ));
    }
}
