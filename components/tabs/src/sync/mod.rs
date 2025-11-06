/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::info;

// Our storage layer stores `TabsRecord` directly, and our tests etc want the tab.
pub use record::{TabsRecord, TabsRecordTab};

mod bridge;
mod engine;
pub(crate) mod record;

pub use bridge::TabsBridgedEngine;
pub use engine::{get_registered_sync_engine, TabsEngine};

// From https://searchfox.org/mozilla-central/rev/ea63a0888d406fae720cf24f4727d87569a8cab5/services/sync/modules/constants.js#75
const URI_LENGTH_MAX: usize = 65536;

const MAX_TITLE_CHAR_LENGTH: usize = 512; // We put an upper limit on title sizes for tabs to reduce memory
const MAX_PAYLOAD_SIZE: usize = 2 * 1024 * 1024; // 2MB, server now 2.5MB? Maybe going to 10? This should come from server config somehow.
                                                 // How many entries we keep in the `urlHistory` field. In practice only 1 ever supplied, so trimming
                                                 // this many is always a noop, but we should maybe bring this back?
const TAB_ENTRIES_LIMIT: usize = 5;

// We try our best to fit as many tabs in a payload as possible, this includes
// limiting the url history entries, title character count and finally drop enough tabs
// until we have small enough payload that the server will accept
pub fn prepare_for_upload(record: &mut TabsRecord) {
    let mut sanitized_tabs = std::mem::take(&mut record.tabs)
        .into_iter()
        .filter_map(|mut tab| {
            if tab.url_history.is_empty() || !is_url_syncable(&tab.url_history[0]) {
                return None;
            }
            let mut sanitized_history = Vec::with_capacity(TAB_ENTRIES_LIMIT);
            for url in tab.url_history {
                if sanitized_history.len() == TAB_ENTRIES_LIMIT {
                    break;
                }
                if is_url_syncable(&url) {
                    sanitized_history.push(url);
                }
            }
            tab.url_history = sanitized_history;
            // Potentially truncate the title to some limit
            tab.title = slice_up_to(tab.title, MAX_TITLE_CHAR_LENGTH);
            Some(tab)
        })
        .collect::<Vec<_>>();
    // Sort the tabs so when we trim tabs it's the oldest tabs
    sanitized_tabs.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    // deduct tab group and window info from the total.
    let used = payload_support::compute_serialized_size(&record.windows)
        .unwrap_or_default()
        .saturating_add(
            payload_support::compute_serialized_size(&record.tab_groups).unwrap_or_default(),
        );
    let size = MAX_PAYLOAD_SIZE.saturating_sub(used);
    trim_tabs_length(&mut sanitized_tabs, size);
    record.tabs = sanitized_tabs;
    info!("prepare_for_upload found {} tabs", record.tabs.len());
}

// Try to keep in sync with https://searchfox.org/mozilla-central/rev/2ad13433da20a0749e1e9a10ec0ab49b987c2c8e/modules/libpref/init/all.js#3927
fn is_url_syncable(url: &str) -> bool {
    url.len() <= URI_LENGTH_MAX
        && !(url.starts_with("about:")
            || url.starts_with("resource:")
            || url.starts_with("chrome:")
            || url.starts_with("wyciwyg:")
            || url.starts_with("blob:")
            || url.starts_with("file:")
            || url.starts_with("moz-extension:")
            || url.starts_with("data:"))
}

/// Trim the amount of tabs in a list to fit the specified memory size.
/// If trimming the tab length fails for some reason, just return the untrimmed tabs.
fn trim_tabs_length(tabs: &mut Vec<TabsRecordTab>, payload_size_max_bytes: usize) {
    if let Some(count) = payload_support::try_fit_items(tabs, payload_size_max_bytes).as_some() {
        tabs.truncate(count.get());
    }
}

// Similar to places/utils.js
// This method ensures we safely truncate a string up to a certain max_len while
// respecting char bounds to prevent rust panics. If we do end up truncating, we
// append an ellipsis to the string
fn slice_up_to(s: String, max_len: usize) -> String {
    if max_len >= s.len() {
        return s;
    }

    let ellipsis = '\u{2026}';
    // Ensure we leave space for the ellipsis while still being under the max
    let mut idx = max_len - ellipsis.len_utf8();
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    let mut new_str = s[..idx].to_string();
    new_str.push(ellipsis);
    new_str
}

#[cfg(test)]
mod tests {
    use super::*;
    use payload_support::compute_serialized_size;

    #[test]
    fn test_is_url_syncable() {
        assert!(is_url_syncable("https://bobo.com"));
        assert!(is_url_syncable("ftp://bobo.com"));
        assert!(!is_url_syncable("about:blank"));
        // XXX - this smells wrong - we should insist on a valid complete URL?
        assert!(is_url_syncable("aboutbobo.com"));
        assert!(!is_url_syncable("file:///Users/eoger/bobo"));
    }

    #[test]
    fn test_trimming_tab_title() {
        error_support::init_for_tests();
        let mut record = TabsRecord {
            id: "me".to_string(),
            client_name: "name".to_string(),
            tabs: vec![TabsRecordTab {
                title: "a".repeat(MAX_TITLE_CHAR_LENGTH + 10), // Fill a string more than max
                url_history: vec!["https://foo.bar".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };
        prepare_for_upload(&mut record);

        let ellipsis_char = '\u{2026}';
        let mut truncated_title = "a".repeat(MAX_TITLE_CHAR_LENGTH - ellipsis_char.len_utf8());
        truncated_title.push(ellipsis_char);
        assert_eq!(
            record.tabs,
            vec![
                // title trimmed to 50 characters
                TabsRecordTab {
                    title: truncated_title, // title was trimmed to only max char length
                    url_history: vec!["https://foo.bar".to_owned()],
                    ..Default::default()
                },
            ]
        );
    }

    #[test]
    fn test_utf8_safe_title_trim() {
        error_support::init_for_tests();
        let mut record = TabsRecord {
            id: "me".to_string(),
            client_name: "name".to_string(),
            tabs: vec![
                TabsRecordTab {
                    title: "😍".repeat(MAX_TITLE_CHAR_LENGTH + 10), // Fill a string more than max
                    url_history: vec!["https://foo.bar".to_owned()],
                    ..Default::default()
                },
                TabsRecordTab {
                    title: "を".repeat(MAX_TITLE_CHAR_LENGTH + 5), // Fill a string more than max
                    url_history: vec!["https://foo_jp.bar".to_owned()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        prepare_for_upload(&mut record);
        // check we truncated correctly.
        let ellipsis_char = '\u{2026}';
        // (MAX_TITLE_CHAR_LENGTH - ellipsis / "😍" bytes)
        let mut truncated_title = "😍".repeat(127);
        // (MAX_TITLE_CHAR_LENGTH - ellipsis / "を" bytes)
        let mut truncated_jp_title = "を".repeat(169);
        truncated_title.push(ellipsis_char);
        truncated_jp_title.push(ellipsis_char);
        assert_eq!(
            record.tabs,
            vec![
                TabsRecordTab {
                    title: truncated_title, // title was trimmed to only max char length
                    url_history: vec!["https://foo.bar".to_owned()],
                    ..Default::default()
                },
                TabsRecordTab {
                    title: truncated_jp_title, // title was trimmed to only max char length
                    url_history: vec!["https://foo_jp.bar".to_owned()],
                    ..Default::default()
                },
            ]
        );
        // We should be less than max
        assert!(record.tabs[0].title.chars().count() <= MAX_TITLE_CHAR_LENGTH);
        assert!(record.tabs[1].title.chars().count() <= MAX_TITLE_CHAR_LENGTH);
    }

    #[test]
    fn test_trim_tabs_length() {
        error_support::init_for_tests();
        let example_tab = TabsRecordTab {
            title: "❤️".repeat(MAX_TITLE_CHAR_LENGTH),
            url_history: vec!["➡️".repeat(250)],
            icon: Some("☺️".repeat(250)),
            ..Default::default()
        };
        let mut record = TabsRecord {
            id: "me".to_string(),
            client_name: "name".to_string(),
            ..Default::default()
        };

        // Given the example, we know that we can fit 440 tabs of this size.
        for _ in 0..440 {
            record.tabs.push(example_tab.clone());
        }
        let had = record.tabs.len();
        prepare_for_upload(&mut record);
        assert_eq!(record.tabs.len(), had);
        assert!(compute_serialized_size(&record).unwrap() <= MAX_PAYLOAD_SIZE);
        // but one more does not fit.
        record.tabs.push(example_tab.clone());
        prepare_for_upload(&mut record);
        assert_eq!(record.tabs.len(), had);
        assert!(compute_serialized_size(&record).unwrap() <= MAX_PAYLOAD_SIZE);
    }

    #[test]
    fn test_prepare_local_tabs_for_upload() {
        error_support::init_for_tests();
        let mut record = TabsRecord {
            id: "me".to_string(),
            client_name: "name".to_string(),
            tabs: vec![
                TabsRecordTab {
                    url_history: vec!["about:blank".to_owned(), "https://foo.bar".to_owned()],
                    ..Default::default()
                },
                TabsRecordTab {
                    url_history: vec![
                        "https://foo.bar".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                        "about:blank".to_owned(),
                    ],
                    ..Default::default()
                },
                TabsRecordTab {
                    url_history: vec![
                        "https://foo.bar".to_owned(),
                        "about:blank".to_owned(),
                        "https://foo2.bar".to_owned(),
                        "https://foo3.bar".to_owned(),
                        "https://foo4.bar".to_owned(),
                        "https://foo5.bar".to_owned(),
                        "https://foo6.bar".to_owned(),
                    ],
                    ..Default::default()
                },
                TabsRecordTab {
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        prepare_for_upload(&mut record);
        assert_eq!(
            record.tabs,
            vec![
                TabsRecordTab {
                    url_history: vec!["https://foo.bar".to_owned()],
                    ..Default::default()
                },
                TabsRecordTab {
                    url_history: vec![
                        "https://foo.bar".to_owned(),
                        "https://foo2.bar".to_owned(),
                        "https://foo3.bar".to_owned(),
                        "https://foo4.bar".to_owned(),
                        "https://foo5.bar".to_owned()
                    ],
                    ..Default::default()
                },
            ]
        );
    }
}
