/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::api::matcher::{self, search_frecent, SearchParams};
pub use crate::api::places_api::places_api_new;
pub use crate::error::{warn, Result};
pub use crate::error::{ApiResult, PlacesApiError};
pub use crate::import::common::HistoryMigrationResult;
use crate::import::import_ios_history;
use crate::storage;
use crate::storage::bookmarks;
pub use crate::storage::bookmarks::BookmarkPosition;
pub use crate::storage::history_metadata::{
    DocumentType, HistoryHighlight, HistoryHighlightWeights, HistoryMetadata,
    HistoryMetadataObservation, HistoryMetadataPageMissingBehavior,
    NoteHistoryMetadataObservationOptions,
};
pub use crate::storage::RunMaintenanceMetrics;
use crate::storage::{history, history_metadata};
use crate::types::VisitTransitionSet;
use crate::ConnectionType;
use crate::VisitObservation;
use crate::VisitType;
use crate::{PlacesApi, PlacesDb};
use error_support::handle_error;
use interrupt_support::register_interrupt;
pub use interrupt_support::SqlInterruptHandle;
use parking_lot::Mutex;
use std::sync::{Arc, Weak};
pub use sync_guid::Guid;
pub use types::Timestamp as PlacesTimestamp;
pub use url::Url;

// From https://searchfox.org/mozilla-central/rev/1674b86019a96f076e0f98f1d0f5f3ab9d4e9020/browser/components/newtab/lib/TopSitesFeed.jsm#87
const SKIP_ONE_PAGE_FRECENCY_THRESHOLD: i64 = 101 + 1;

// `bookmarks::InsertableItem` is clear for Rust code, but just `InsertableItem` is less
// clear in the UDL - so change some of the type names.
pub type InsertableBookmarkItem = crate::storage::bookmarks::InsertableItem;
pub type InsertableBookmarkFolder = crate::storage::bookmarks::InsertableFolder;
pub type InsertableBookmarkSeparator = crate::storage::bookmarks::InsertableSeparator;
pub use crate::storage::bookmarks::InsertableBookmark;

pub use crate::storage::bookmarks::BookmarkUpdateInfo;

// And types used when fetching items.
pub type BookmarkItem = crate::storage::bookmarks::fetch::Item;
pub type BookmarkFolder = crate::storage::bookmarks::fetch::Folder;
pub type BookmarkSeparator = crate::storage::bookmarks::fetch::Separator;
pub use crate::storage::bookmarks::fetch::BookmarkData;

uniffi::custom_type!(Url, String, {
    remote,
    try_lift: |val| {
        match Url::parse(val.as_str()) {
            Ok(url) => Ok(url),
            Err(e) => Err(PlacesApiError::UrlParseFailed {
                reason: e.to_string(),
            }
            .into()),
        }
    },
    lower: |obj| obj.into(),
});

uniffi::custom_type!(PlacesTimestamp, i64, {
    remote,
    try_lift: |val| Ok(PlacesTimestamp(val as u64)),
    lower: |obj| obj.as_millis() as i64,
});

uniffi::custom_type!(VisitTransitionSet, i32, {
    try_lift: |val| {
        Ok(VisitTransitionSet::from_u16(val as u16).expect("Bug: Invalid VisitTransitionSet"))
    },
    lower: |obj| VisitTransitionSet::into_u16(obj) as i32,
});

uniffi::custom_type!(Guid, String, {
    remote,
    try_lift: |val| Ok(Guid::new(val.as_str())),
    lower: |obj| obj.into(),
});

// Check for multiple write connections open at the same time
//
// One potential cause of #5040 is that Fenix is somehow opening multiiple write connections to
// the places DB.  This code tests if that's happening and reports an error if so.
lazy_static::lazy_static! {
    static ref READ_WRITE_CONNECTIONS: Mutex<Vec<Weak<PlacesConnection>>> = Mutex::new(Vec::new());
    static ref SYNC_CONNECTIONS: Mutex<Vec<Weak<PlacesConnection>>> = Mutex::new(Vec::new());
}

impl PlacesApi {
    #[handle_error(crate::Error)]
    pub fn new_connection(&self, conn_type: ConnectionType) -> ApiResult<Arc<PlacesConnection>> {
        let db = self.open_connection(conn_type)?;
        let connection = Arc::new(PlacesConnection::new(db));
        register_interrupt(Arc::<PlacesConnection>::downgrade(&connection));
        Ok(connection)
    }
}

pub struct PlacesConnection {
    db: Mutex<PlacesDb>,
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl PlacesConnection {
    pub fn new(db: PlacesDb) -> Self {
        Self {
            interrupt_handle: db.new_interrupt_handle(),
            db: Mutex::new(db),
        }
    }

    // A helper that gets the connection from the mutex and converts errors.
    fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&PlacesDb) -> crate::error::Result<T>,
    {
        let conn = self.db.lock();
        f(&conn)
    }

    // pass the SqlInterruptHandle as an object through Uniffi
    pub fn new_interrupt_handle(&self) -> Arc<SqlInterruptHandle> {
        Arc::clone(&self.interrupt_handle)
    }

    #[handle_error(crate::Error)]
    pub fn get_latest_history_metadata_for_url(
        &self,
        url: Url,
    ) -> ApiResult<Option<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_latest_for_url(conn, &url))
    }

    #[handle_error(crate::Error)]
    pub fn get_history_metadata_between(
        &self,
        start: PlacesTimestamp,
        end: PlacesTimestamp,
    ) -> ApiResult<Vec<HistoryMetadata>> {
        self.with_conn(|conn| {
            history_metadata::get_between(conn, start.as_millis_i64(), end.as_millis_i64())
        })
    }

    #[handle_error(crate::Error)]
    pub fn get_history_metadata_since(
        &self,
        start: PlacesTimestamp,
    ) -> ApiResult<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_since(conn, start.as_millis_i64()))
    }

    #[handle_error(crate::Error)]
    pub fn get_most_recent_history_metadata(&self, limit: i32) -> ApiResult<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_most_recent(conn, limit))
    }

    #[handle_error(crate::Error)]
    pub fn get_most_recent_search_entries_in_history_metadata(
        &self,
        limit: i32,
    ) -> ApiResult<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_most_recent_search_entries(conn, limit))
    }

    #[handle_error(crate::Error)]
    pub fn query_history_metadata(
        &self,
        query: String,
        limit: i32,
    ) -> ApiResult<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::query(conn, query.as_str(), limit))
    }

    #[handle_error(crate::Error)]
    pub fn get_history_highlights(
        &self,
        weights: HistoryHighlightWeights,
        limit: i32,
    ) -> ApiResult<Vec<HistoryHighlight>> {
        self.with_conn(|conn| history_metadata::get_highlights(conn, weights, limit))
    }

    #[handle_error(crate::Error)]
    pub fn note_history_metadata_observation(
        &self,
        data: HistoryMetadataObservation,
        options: NoteHistoryMetadataObservationOptions,
    ) -> ApiResult<()> {
        // odd historical naming discrepancy - public function is "note_*", impl is "apply_*"
        self.with_conn(|conn| history_metadata::apply_metadata_observation(conn, data, options))
    }

    #[handle_error(crate::Error)]
    pub fn metadata_delete_older_than(&self, older_than: PlacesTimestamp) -> ApiResult<()> {
        self.with_conn(|conn| history_metadata::delete_older_than(conn, older_than.as_millis_i64()))
    }

    #[handle_error(crate::Error)]
    pub fn metadata_delete(
        &self,
        url: Url,
        referrer_url: Option<Url>,
        search_term: Option<String>,
    ) -> ApiResult<()> {
        self.with_conn(|conn| {
            history_metadata::delete_metadata(
                conn,
                &url,
                referrer_url.as_ref(),
                search_term.as_deref(),
            )
        })
    }

    #[handle_error(crate::Error)]
    pub fn metadata_delete_search_terms(&self) -> ApiResult<()> {
        self.with_conn(history_metadata::delete_all_metadata_for_search)
    }

    /// Add an observation to the database.
    #[handle_error(crate::Error)]
    pub fn apply_observation(&self, visit: VisitObservation) -> ApiResult<()> {
        self.with_conn(|conn| history::apply_observation(conn, visit))?;
        Ok(())
    }

    #[handle_error(crate::Error)]
    pub fn get_visited_urls_in_range(
        &self,
        start: PlacesTimestamp,
        end: PlacesTimestamp,
        include_remote: bool,
    ) -> ApiResult<Vec<Url>> {
        self.with_conn(|conn| {
            let urls = history::get_visited_urls(conn, start, end, include_remote)?
                .iter()
                // Turn the list of strings into valid Urls
                .filter_map(|s| Url::parse(s).ok())
                .collect::<Vec<_>>();
            Ok(urls)
        })
    }

    #[handle_error(crate::Error)]
    pub fn get_visit_infos(
        &self,
        start_date: PlacesTimestamp,
        end_date: PlacesTimestamp,
        exclude_types: VisitTransitionSet,
    ) -> ApiResult<Vec<HistoryVisitInfo>> {
        self.with_conn(|conn| history::get_visit_infos(conn, start_date, end_date, exclude_types))
    }

    #[handle_error(crate::Error)]
    pub fn get_visit_count(&self, exclude_types: VisitTransitionSet) -> ApiResult<i64> {
        self.with_conn(|conn| history::get_visit_count(conn, exclude_types))
    }

    #[handle_error(crate::Error)]
    pub fn get_visit_count_for_host(
        &self,
        host: String,
        before: PlacesTimestamp,
        exclude_types: VisitTransitionSet,
    ) -> ApiResult<i64> {
        self.with_conn(|conn| {
            history::get_visit_count_for_host(conn, host.as_str(), before, exclude_types)
        })
    }

    #[handle_error(crate::Error)]
    pub fn get_visit_page(
        &self,
        offset: i64,
        count: i64,
        exclude_types: VisitTransitionSet,
    ) -> ApiResult<Vec<HistoryVisitInfo>> {
        self.with_conn(|conn| history::get_visit_page(conn, offset, count, exclude_types))
    }

    #[handle_error(crate::Error)]
    pub fn get_visit_page_with_bound(
        &self,
        bound: i64,
        offset: i64,
        count: i64,
        exclude_types: VisitTransitionSet,
    ) -> ApiResult<HistoryVisitInfosWithBound> {
        self.with_conn(|conn| {
            history::get_visit_page_with_bound(conn, bound, offset, count, exclude_types)
        })
    }

    // This is identical to get_visited in history.rs but takes a list of strings instead of urls
    // This is necessary b/c we still need to return 'false' for bad URLs which prevents us from
    // parsing/filtering them before reaching the history layer
    #[handle_error(crate::Error)]
    pub fn get_visited(&self, urls: Vec<String>) -> ApiResult<Vec<bool>> {
        let iter = urls.into_iter();
        let mut result = vec![false; iter.len()];
        let url_idxs = iter
            .enumerate()
            .filter_map(|(idx, s)| Url::parse(&s).ok().map(|url| (idx, url)))
            .collect::<Vec<_>>();
        self.with_conn(|conn| history::get_visited_into(conn, &url_idxs, &mut result))?;
        Ok(result)
    }

    #[handle_error(crate::Error)]
    pub fn delete_visits_for(&self, url: String) -> ApiResult<()> {
        self.with_conn(|conn| {
            let guid = match Url::parse(&url) {
                Ok(url) => history::url_to_guid(conn, &url)?,
                Err(e) => {
                    warn!("Invalid URL passed to places_delete_visits_for, {}", e);
                    history::href_to_guid(conn, url.clone().as_str())?
                }
            };
            if let Some(guid) = guid {
                history::delete_visits_for(conn, &guid)?;
            }
            Ok(())
        })
    }

    #[handle_error(crate::Error)]
    pub fn delete_visits_between(
        &self,
        start: PlacesTimestamp,
        end: PlacesTimestamp,
    ) -> ApiResult<()> {
        self.with_conn(|conn| history::delete_visits_between(conn, start, end))
    }

    #[handle_error(crate::Error)]
    pub fn delete_visit(&self, url: String, timestamp: PlacesTimestamp) -> ApiResult<()> {
        self.with_conn(|conn| {
            match Url::parse(&url) {
                Ok(url) => {
                    history::delete_place_visit_at_time(conn, &url, timestamp)?;
                }
                Err(e) => {
                    warn!("Invalid URL passed to places_delete_visit, {}", e);
                    history::delete_place_visit_at_time_by_href(conn, url.as_str(), timestamp)?;
                }
            };
            Ok(())
        })
    }

    #[handle_error(crate::Error)]
    pub fn get_top_frecent_site_infos(
        &self,
        num_items: i32,
        threshold_option: FrecencyThresholdOption,
    ) -> ApiResult<Vec<TopFrecentSiteInfo>> {
        self.with_conn(|conn| {
            crate::storage::history::get_top_frecent_site_infos(
                conn,
                num_items,
                threshold_option.value(),
            )
        })
    }
    // deletes all history and updates the sync metadata to only sync after
    // most recent visit to prevent further syncing of older data
    #[handle_error(crate::Error)]
    pub fn delete_everything_history(&self) -> ApiResult<()> {
        history::delete_everything(&self.db.lock())
    }

    #[handle_error(crate::Error)]
    pub fn run_maintenance_prune(
        &self,
        db_size_limit: u32,
        prune_limit: u32,
    ) -> ApiResult<RunMaintenanceMetrics> {
        self.with_conn(|conn| storage::run_maintenance_prune(conn, db_size_limit, prune_limit))
    }

    #[handle_error(crate::Error)]
    pub fn run_maintenance_vacuum(&self) -> ApiResult<()> {
        self.with_conn(storage::run_maintenance_vacuum)
    }

    #[handle_error(crate::Error)]
    pub fn run_maintenance_optimize(&self) -> ApiResult<()> {
        self.with_conn(storage::run_maintenance_optimize)
    }

    #[handle_error(crate::Error)]
    pub fn run_maintenance_checkpoint(&self) -> ApiResult<()> {
        self.with_conn(storage::run_maintenance_checkpoint)
    }

    #[handle_error(crate::Error)]
    pub fn query_autocomplete(&self, search: String, limit: i32) -> ApiResult<Vec<SearchResult>> {
        self.with_conn(|conn| {
            search_frecent(
                conn,
                SearchParams {
                    search_string: search,
                    limit: limit as u32,
                },
            )
            .map(|search_results| search_results.into_iter().map(Into::into).collect())
        })
    }

    #[handle_error(crate::Error)]
    pub fn accept_result(&self, search_string: String, url: String) -> ApiResult<()> {
        self.with_conn(|conn| {
            match Url::parse(&url) {
                Ok(url) => {
                    matcher::accept_result(conn, &search_string, &url)?;
                }
                Err(_) => {
                    warn!("Ignoring invalid URL in places_accept_result");
                    return Ok(());
                }
            };
            Ok(())
        })
    }

    #[handle_error(crate::Error)]
    pub fn match_url(&self, query: String) -> ApiResult<Option<Url>> {
        self.with_conn(|conn| matcher::match_url(conn, query))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_get_tree(&self, item_guid: &Guid) -> ApiResult<Option<BookmarkItem>> {
        self.with_conn(|conn| bookmarks::fetch::fetch_tree(conn, item_guid))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_get_by_guid(
        &self,
        guid: &Guid,
        get_direct_children: bool,
    ) -> ApiResult<Option<BookmarkItem>> {
        self.with_conn(|conn| {
            let bookmark = bookmarks::fetch::fetch_bookmark(conn, guid, get_direct_children)?;
            Ok(bookmark)
        })
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_get_all_with_url(&self, url: String) -> ApiResult<Vec<BookmarkItem>> {
        self.with_conn(|conn| {
            // XXX - We should return the exact type - ie, BookmarkData rather than BookmarkItem.
            match Url::parse(&url) {
                Ok(url) => Ok(bookmarks::fetch::fetch_bookmarks_by_url(conn, &url)?
                    .into_iter()
                    .map(|b| BookmarkItem::Bookmark { b })
                    .collect::<Vec<BookmarkItem>>()),
                Err(e) => {
                    // There are no bookmarks with the URL if it's invalid.
                    warn!("Invalid URL passed to bookmarks_get_all_with_url, {}", e);
                    Ok(Vec::<BookmarkItem>::new())
                }
            }
        })
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_search(&self, query: String, limit: i32) -> ApiResult<Vec<BookmarkItem>> {
        self.with_conn(|conn| {
            // XXX - We should return the exact type - ie, BookmarkData rather than BookmarkItem.
            Ok(
                bookmarks::fetch::search_bookmarks(conn, query.as_str(), limit as u32)?
                    .into_iter()
                    .map(|b| BookmarkItem::Bookmark { b })
                    .collect(),
            )
        })
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_get_recent(&self, limit: i32) -> ApiResult<Vec<BookmarkItem>> {
        self.with_conn(|conn| {
            // XXX - We should return the exact type - ie, BookmarkData rather than BookmarkItem.
            Ok(bookmarks::fetch::recent_bookmarks(conn, limit as u32)?
                .into_iter()
                .map(|b| BookmarkItem::Bookmark { b })
                .collect())
        })
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_delete(&self, id: Guid) -> ApiResult<bool> {
        self.with_conn(|conn| bookmarks::delete_bookmark(conn, &id))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_delete_everything(&self) -> ApiResult<()> {
        self.with_conn(bookmarks::delete_everything)
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_get_url_for_keyword(&self, keyword: String) -> ApiResult<Option<Url>> {
        self.with_conn(|conn| bookmarks::bookmarks_get_url_for_keyword(conn, keyword.as_str()))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_insert(&self, data: InsertableBookmarkItem) -> ApiResult<Guid> {
        self.with_conn(|conn| bookmarks::insert_bookmark(conn, data))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_update(&self, item: BookmarkUpdateInfo) -> ApiResult<()> {
        self.with_conn(|conn| bookmarks::update_bookmark_from_info(conn, item))
    }

    #[handle_error(crate::Error)]
    pub fn bookmarks_count_bookmarks_in_trees(&self, guids: &[Guid]) -> ApiResult<u32> {
        self.with_conn(|conn| bookmarks::count_bookmarks_in_trees(conn, guids))
    }

    #[handle_error(crate::Error)]
    pub fn places_history_import_from_ios(
        &self,
        db_path: String,
        last_sync_timestamp: i64,
    ) -> ApiResult<HistoryMigrationResult> {
        self.with_conn(|conn| import_ios_history(conn, &db_path, last_sync_timestamp))
    }
}

impl AsRef<SqlInterruptHandle> for PlacesConnection {
    fn as_ref(&self) -> &SqlInterruptHandle {
        &self.interrupt_handle
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HistoryVisitInfo {
    pub url: Url,
    pub title: Option<String>,
    pub timestamp: PlacesTimestamp,
    pub visit_type: VisitType,
    pub is_hidden: bool,
    pub preview_image_url: Option<Url>,
    pub is_remote: bool,
}
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryVisitInfosWithBound {
    pub infos: Vec<HistoryVisitInfo>,
    pub bound: i64,
    pub offset: i64,
}

pub struct TopFrecentSiteInfo {
    pub url: Url,
    pub title: Option<String>,
}

pub enum FrecencyThresholdOption {
    None,
    SkipOneTimePages,
}

impl FrecencyThresholdOption {
    fn value(&self) -> i64 {
        match self {
            FrecencyThresholdOption::None => 0,
            FrecencyThresholdOption::SkipOneTimePages => SKIP_ONE_PAGE_FRECENCY_THRESHOLD,
        }
    }
}

pub struct SearchResult {
    pub url: Url,
    pub title: String,
    pub frecency: i64,
}

// Exists just to convince uniffi to generate `liftSequence*` helpers!
pub struct Dummy {
    pub md: Option<Vec<HistoryMetadata>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::new_mem_connection;

    #[test]
    fn test_accept_result_with_invalid_url() {
        let conn = PlacesConnection::new(new_mem_connection());
        let invalid_url = "http://1234.56.78.90".to_string();
        assert!(PlacesConnection::accept_result(&conn, "ample".to_string(), invalid_url).is_ok());
    }

    #[test]
    fn test_bookmarks_get_all_with_url_with_invalid_url() {
        let conn = PlacesConnection::new(new_mem_connection());
        let invalid_url = "http://1234.56.78.90".to_string();
        assert!(PlacesConnection::bookmarks_get_all_with_url(&conn, invalid_url).is_ok());
    }
}
