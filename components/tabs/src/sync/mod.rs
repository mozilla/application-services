/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{info, warn};
use record::TabsRecordTab;

pub(crate) mod bridge;
pub(crate) mod engine;
pub(crate) mod record;

// From https://searchfox.org/mozilla-central/rev/ea63a0888d406fae720cf24f4727d87569a8cab5/services/sync/modules/constants.js#75
const URI_LENGTH_MAX: usize = 65536;

const MAX_TITLE_CHAR_LENGTH: usize = 512; // We put an upper limit on title sizes for tabs to reduce memory
const MAX_PAYLOAD_SIZE: usize = 2 * 1024 * 1024; // 2MB, server now 2.5MB? Maybe going to 10? This should come from server config somehow.
                                                 // How many entries we keep in the `urlHistory` field. In practice only 1 ever supplied, so trimming
                                                 // this many is always a noop, but we should maybe bring this back?
const TAB_ENTRIES_LIMIT: usize = 5;

impl crate::storage::TabsStorage {
    // We try our best to fit as many tabs in a payload as possible, this includes
    // limiting the url history entries, title character count and finally drop enough tabs
    // until we have small enough payload that the server will accept
    pub fn prepare_local_tabs_for_upload(&self) -> Option<Vec<TabsRecordTab>> {
        if let Some(tabs) = self.local_tabs.borrow().as_ref() {
            info!("prepare_local_tabs_for_upload found {} tabs", tabs.len());
            let mut sanitized_tabs: Vec<TabsRecordTab> = tabs
                .iter()
                .cloned()
                .filter_map(|remote_tab| {
                    if remote_tab.url_history.is_empty()
                        || !is_url_syncable(&remote_tab.url_history[0])
                    {
                        return None;
                    }
                    let mut tab = remote_tab.to_record_tab();
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
                .collect();
            // Sort the tabs so when we trim tabs it's the oldest tabs
            sanitized_tabs.sort_by(|a, b| b.last_used.cmp(&a.last_used));
            trim_tabs_length(&mut sanitized_tabs, MAX_PAYLOAD_SIZE);
            info!(
                "prepare_local_tabs_for_upload found {} tabs",
                sanitized_tabs.len()
            );
            Some(sanitized_tabs)
        } else {
            // It's a less than ideal outcome if at startup (or any time) we are asked to
            // sync tabs before the app has told us what the tabs are, so make noise.
            warn!("prepare_local_tabs_for_upload - have no local tabs");
            None
        }
    }
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
    use crate::storage::{RemoteTab, TabsStorage};
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
        let mut storage = TabsStorage::new_with_mem_path("test_prepare_local_tabs_for_upload");
        assert_eq!(storage.prepare_local_tabs_for_upload(), None);
        storage.update_local_state(vec![RemoteTab {
            title: "a".repeat(MAX_TITLE_CHAR_LENGTH + 10), // Fill a string more than max
            url_history: vec!["https://foo.bar".to_owned()],
            ..Default::default()
        }]);
        let ellipsis_char = '\u{2026}';
        let mut truncated_title = "a".repeat(MAX_TITLE_CHAR_LENGTH - ellipsis_char.len_utf8());
        truncated_title.push(ellipsis_char);
        assert_eq!(
            storage.prepare_local_tabs_for_upload(),
            Some(vec![
                // title trimmed to 50 characters
                TabsRecordTab {
                    title: truncated_title, // title was trimmed to only max char length
                    url_history: vec!["https://foo.bar".to_owned()],
                    ..Default::default()
                },
            ])
        );
    }
    #[test]
    fn test_utf8_safe_title_trim() {
        error_support::init_for_tests();
        let mut storage = TabsStorage::new_with_mem_path("test_prepare_local_tabs_for_upload");
        assert_eq!(storage.prepare_local_tabs_for_upload(), None);
        storage.update_local_state(vec![
            RemoteTab {
                title: "😍".repeat(MAX_TITLE_CHAR_LENGTH + 10), // Fill a string more than max
                url_history: vec!["https://foo.bar".to_owned()],
                ..Default::default()
            },
            RemoteTab {
                title: "を".repeat(MAX_TITLE_CHAR_LENGTH + 5), // Fill a string more than max
                url_history: vec!["https://foo_jp.bar".to_owned()],
                ..Default::default()
            },
        ]);
        let ellipsis_char = '\u{2026}';
        // (MAX_TITLE_CHAR_LENGTH - ellipsis / "😍" bytes)
        let mut truncated_title = "😍".repeat(127);
        // (MAX_TITLE_CHAR_LENGTH - ellipsis / "を" bytes)
        let mut truncated_jp_title = "を".repeat(169);
        truncated_title.push(ellipsis_char);
        truncated_jp_title.push(ellipsis_char);
        let remote_tabs = storage.prepare_local_tabs_for_upload().unwrap();
        assert_eq!(
            remote_tabs,
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
        assert!(remote_tabs[0].title.chars().count() <= MAX_TITLE_CHAR_LENGTH);
        assert!(remote_tabs[1].title.chars().count() <= MAX_TITLE_CHAR_LENGTH);
    }
    #[test]
    fn test_trim_tabs_length() {
        error_support::init_for_tests();
        let mut storage = TabsStorage::new_with_mem_path("test_prepare_local_tabs_for_upload");
        assert_eq!(storage.prepare_local_tabs_for_upload(), None);
        let example_tab = RemoteTab {
            title: "❤️".repeat(MAX_TITLE_CHAR_LENGTH),
            url_history: vec!["➡️".repeat(250)],
            icon: Some("☺️".repeat(250)),
            ..Default::default()
        };
        let mut too_many_tabs = vec![];
        // Given the example, we know that we can fit 440 tabs of this size.
        for _ in 0..440 {
            too_many_tabs.push(example_tab.clone());
        }
        storage.update_local_state(too_many_tabs.clone());
        let tabs_to_upload = storage.prepare_local_tabs_for_upload().unwrap();
        assert_eq!(tabs_to_upload.len(), too_many_tabs.len());
        assert!(compute_serialized_size(&tabs_to_upload).unwrap() <= MAX_PAYLOAD_SIZE);
        // but one more does not fit.
        too_many_tabs.push(example_tab.clone());
        storage.update_local_state(too_many_tabs.clone());
        let tabs_to_upload = storage.prepare_local_tabs_for_upload().unwrap();
        assert_eq!(tabs_to_upload.len(), too_many_tabs.len() - 1);
        assert!(compute_serialized_size(&tabs_to_upload).unwrap() <= MAX_PAYLOAD_SIZE);
    }
}
