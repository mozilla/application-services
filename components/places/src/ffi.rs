/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::api::matcher::{self, search_frecent, SearchParams};
use crate::api::places_api::places_api_new;
use crate::error::PlacesError;
use crate::import::fennec::import_bookmarks;
use crate::import::fennec::import_history;
use crate::import::fennec::import_pinned_sites;
use crate::storage;
use crate::storage::bookmarks;
use crate::storage::bookmarks::BookmarkPosition;
use crate::storage::history_metadata::{
    DocumentType, HistoryHighlight, HistoryHighlightWeights, HistoryMetadata,
    HistoryMetadataObservation,
};
use crate::storage::{history, history_metadata};
use crate::types::{BookmarkType, VisitTransitionSet};
use crate::ConnectionType;
use crate::VisitObservation;
use crate::VisitTransition;
use crate::{PlacesApi, PlacesDb};
use parking_lot::Mutex;
use sql_support::SqlInterruptHandle;
use std::sync::Arc;
use sync_guid::Guid;
use types::Timestamp;
use url::Url;

// From https://searchfox.org/mozilla-central/rev/1674b86019a96f076e0f98f1d0f5f3ab9d4e9020/browser/components/newtab/lib/TopSitesFeed.jsm#87
const SKIP_ONE_PAGE_FRECENCY_THRESHOLD: i64 = 101 + 1;

// All of our functions in this module use a `Result` type with the error we throw over
// the FFI.
type Result<T> = std::result::Result<T, PlacesError>;

// `bookmarks::InsertableItem` is clear for Rust code, but just `InsertableItem` is less
// clear in the UDL - so change some of the type names.
type InsertableBookmarkItem = crate::storage::bookmarks::InsertableItem;
type InsertableBookmarkFolder = crate::storage::bookmarks::InsertableFolder;
type InsertableBookmarkSeparator = crate::storage::bookmarks::InsertableSeparator;
use crate::storage::bookmarks::InsertableBookmark;

use crate::storage::bookmarks::BookmarkUpdateInfo;

// And types used when fetching items.
type BookmarkItem = crate::storage::bookmarks::fetch::Item;
type BookmarkFolder = crate::storage::bookmarks::fetch::Folder;
type BookmarkSeparator = crate::storage::bookmarks::fetch::Separator;
use crate::storage::bookmarks::fetch::BookmarkData;

impl UniffiCustomTypeWrapper for Url {
    type Wrapped = String;

    fn wrap(val: Self::Wrapped) -> uniffi::Result<url::Url> {
        match Url::parse(val.as_str()) {
            Ok(url) => Ok(url),
            Err(e) => Err(PlacesError::UrlParseFailed(e.to_string()).into()),
        }
    }

    fn unwrap(obj: Self) -> Self::Wrapped {
        obj.into()
    }
}

impl UniffiCustomTypeWrapper for Timestamp {
    type Wrapped = i64;

    fn wrap(val: Self::Wrapped) -> uniffi::Result<Self> {
        Ok(Timestamp(val as u64))
    }

    fn unwrap(obj: Self) -> Self::Wrapped {
        obj.as_millis() as i64
    }
}

impl UniffiCustomTypeWrapper for VisitTransitionSet {
    type Wrapped = i32;

    fn wrap(val: Self::Wrapped) -> uniffi::Result<Self> {
        Ok(VisitTransitionSet::from_u16(val as u16).expect("Bug: Invalid VisitTransitionSet"))
    }

    fn unwrap(obj: Self) -> Self::Wrapped {
        VisitTransitionSet::into_u16(obj) as i32
    }
}

impl UniffiCustomTypeWrapper for Guid {
    type Wrapped = String;

    fn wrap(val: Self::Wrapped) -> uniffi::Result<Guid> {
        Ok(Guid::new(val.as_str()))
    }

    fn unwrap(obj: Self) -> Self::Wrapped {
        obj.into()
    }
}

impl PlacesApi {
    fn new_connection(&self, conn_type: ConnectionType) -> Result<Arc<PlacesConnection>> {
        let db = self.open_connection(conn_type)?;
        let connection = PlacesConnection { db: Mutex::new(db) };
        Ok(Arc::new(connection))
    }

    // NOTE: These methods are unused on Android but will remain needed for
    // iOS until we can move them to the sync manager and replace their existing
    // sync engines with ours
    fn history_sync(
        &self,
        key_id: String,
        access_token: String,
        sync_key: String,
        tokenserver_url: Url,
    ) -> Result<String> {
        let root_sync_key = match sync15::KeyBundle::from_ksync_base64(sync_key.as_str()) {
            Ok(key) => Ok(key),
            Err(err) => Err(PlacesError::UnexpectedPlacesException(err.to_string())),
        }?;
        let ping = self.sync_history(
            &sync15::Sync15StorageClientInit {
                key_id,
                access_token,
                tokenserver_url,
            },
            &root_sync_key,
        )?;
        Ok(serde_json::to_string(&ping).unwrap())
    }

    fn bookmarks_sync(
        &self,
        key_id: String,
        access_token: String,
        sync_key: String,
        tokenserver_url: Url,
    ) -> Result<String> {
        let root_sync_key = match sync15::KeyBundle::from_ksync_base64(sync_key.as_str()) {
            Ok(key) => Ok(key),
            Err(err) => Err(PlacesError::UnexpectedPlacesException(err.to_string())),
        }?;
        let ping = self.sync_bookmarks(
            &sync15::Sync15StorageClientInit {
                key_id,
                access_token,
                tokenserver_url,
            },
            &root_sync_key,
        )?;
        Ok(serde_json::to_string(&ping).unwrap())
    }

    fn places_pinned_sites_import_from_fennec(&self, db_path: String) -> Result<Vec<BookmarkItem>> {
        let sites = import_pinned_sites(self, db_path.as_str())?
            .into_iter()
            .map(BookmarkItem::from)
            .collect();
        Ok(sites)
    }

    fn places_history_import_from_fennec(&self, db_path: String) -> Result<String> {
        let metrics = import_history(self, db_path.as_str())?;
        Ok(serde_json::to_string(&metrics)?)
    }

    fn places_bookmarks_import_from_fennec(&self, db_path: String) -> Result<String> {
        let metrics = import_bookmarks(self, db_path.as_str())?;
        Ok(serde_json::to_string(&metrics)?)
    }

    fn places_bookmarks_import_from_ios(&self, db_path: String) -> Result<()> {
        import_bookmarks(self, db_path.as_str())?;
        Ok(())
    }

    fn bookmarks_reset(&self) -> Result<()> {
        self.reset_bookmarks()?;
        Ok(())
    }
}

pub struct PlacesConnection {
    db: Mutex<PlacesDb>,
}

impl PlacesConnection {
    // A helper that gets the connection from the mutex and converts errors.
    fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&PlacesDb) -> crate::error::Result<T>,
    {
        let conn = self.db.lock();
        Ok(f(&conn)?)
    }

    // This should be refactored/removed as part of https://github.com/mozilla/application-services/issues/1684
    // We have to use Arc in the return type to be able to properly
    // pass the SqlInterruptHandle as an object through Uniffi
    fn new_interrupt_handle(&self) -> Result<Arc<SqlInterruptHandle>> {
        Ok(Arc::new(
            self.with_conn(|conn| Ok(conn.new_interrupt_handle()))?,
        ))
    }

    fn get_latest_history_metadata_for_url(&self, url: Url) -> Result<Option<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_latest_for_url(conn, &url))
    }

    fn get_history_metadata_between(
        &self,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Vec<HistoryMetadata>> {
        self.with_conn(|conn| {
            history_metadata::get_between(conn, start.as_millis_i64(), end.as_millis_i64())
        })
    }

    fn get_history_metadata_since(&self, start: Timestamp) -> Result<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_since(conn, start.as_millis_i64()))
    }

    fn query_history_metadata(&self, query: String, limit: i32) -> Result<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::query(conn, query.as_str(), limit))
    }

    fn get_history_highlights(
        &self,
        weights: HistoryHighlightWeights,
        limit: i32,
    ) -> Result<Vec<HistoryHighlight>> {
        self.with_conn(|conn| history_metadata::get_highlights(conn, weights, limit))
    }

    fn note_history_metadata_observation(&self, data: HistoryMetadataObservation) -> Result<()> {
        // odd historical naming discrepency - public function is "note_*", impl is "apply_*"
        self.with_conn(|conn| history_metadata::apply_metadata_observation(conn, data))
    }

    fn metadata_delete_older_than(&self, older_than: Timestamp) -> Result<()> {
        self.with_conn(|conn| history_metadata::delete_older_than(conn, older_than.as_millis_i64()))
    }

    fn metadata_delete(
        &self,
        url: Url,
        referrer_url: Option<Url>,
        search_term: Option<String>,
    ) -> Result<()> {
        self.with_conn(|conn| {
            history_metadata::delete_metadata(
                conn,
                &url,
                referrer_url.as_ref(),
                search_term.as_deref(),
            )
        })
    }

    /// Add an observation to the database.
    fn apply_observation(&self, visit: VisitObservation) -> Result<()> {
        self.with_conn(|conn| history::apply_observation(conn, visit))?;
        Ok(())
    }

    fn get_visited_urls_in_range(
        &self,
        start: Timestamp,
        end: Timestamp,
        include_remote: bool,
    ) -> Result<Vec<Url>> {
        self.with_conn(|conn| {
            let urls = history::get_visited_urls(conn, start, end, include_remote)?
                .iter()
                // Turn the list of strings into valid Urls
                .filter_map(|s| Url::parse(s).ok())
                .collect::<Vec<_>>();
            Ok(urls)
        })
    }

    fn get_visit_infos(
        &self,
        start_date: Timestamp,
        end_date: Timestamp,
        exclude_types: VisitTransitionSet,
    ) -> Result<Vec<HistoryVisitInfo>> {
        self.with_conn(|conn| history::get_visit_infos(conn, start_date, end_date, exclude_types))
    }

    fn get_visit_count(&self, exclude_types: VisitTransitionSet) -> Result<i64> {
        self.with_conn(|conn| history::get_visit_count(conn, exclude_types))
    }

    fn get_visit_page(
        &self,
        offset: i64,
        count: i64,
        exclude_types: VisitTransitionSet,
    ) -> Result<Vec<HistoryVisitInfo>> {
        self.with_conn(|conn| history::get_visit_page(conn, offset, count, exclude_types))
    }

    fn get_visit_page_with_bound(
        &self,
        bound: i64,
        offset: i64,
        count: i64,
        exclude_types: VisitTransitionSet,
    ) -> Result<HistoryVisitInfosWithBound> {
        self.with_conn(|conn| {
            history::get_visit_page_with_bound(conn, bound, offset, count, exclude_types)
        })
    }

    // This is identical to get_visited in history.rs but takes a list of strings instead of urls
    // This is necessary b/c we still need to return 'false' for bad URLs which prevents us from
    // parsing/filtering them before reaching the history layer
    fn get_visited(&self, urls: Vec<String>) -> Result<Vec<bool>> {
        let iter = urls.into_iter();
        let mut result = vec![false; iter.len()];
        let url_idxs = iter
            .enumerate()
            .filter_map(|(idx, s)| Url::parse(&s).ok().map(|url| (idx, url)))
            .collect::<Vec<_>>();
        self.with_conn(|conn| history::get_visited_into(conn, &url_idxs, &mut result))?;
        Ok(result)
    }

    fn delete_visits_for(&self, url: String) -> Result<()> {
        self.with_conn(|conn| {
            let guid = match Url::parse(&url) {
                Ok(url) => history::url_to_guid(conn, &url)?,
                Err(e) => {
                    log::warn!("Invalid URL passed to places_delete_visits_for, {}", e);
                    history::href_to_guid(conn, url.clone().as_str())?
                }
            };
            if let Some(guid) = guid {
                history::delete_visits_for(conn, &guid)?;
            }
            Ok(())
        })
    }

    fn delete_visits_between(&self, start: Timestamp, end: Timestamp) -> Result<()> {
        self.with_conn(|conn| history::delete_visits_between(conn, start, end))
    }

    fn delete_visit(&self, url: String, timestamp: Timestamp) -> Result<()> {
        self.with_conn(|conn| {
            match Url::parse(&url) {
                Ok(url) => {
                    history::delete_place_visit_at_time(conn, &url, timestamp)?;
                }
                Err(e) => {
                    log::warn!("Invalid URL passed to places_delete_visit, {}", e);
                    history::delete_place_visit_at_time_by_href(conn, url.as_str(), timestamp)?;
                }
            };
            Ok(())
        })
    }

    fn get_top_frecent_site_infos(
        &self,
        num_items: i32,
        threshold_option: FrecencyThresholdOption,
    ) -> Result<Vec<TopFrecentSiteInfo>> {
        self.with_conn(|conn| {
            crate::storage::history::get_top_frecent_site_infos(
                conn,
                num_items,
                threshold_option.value(),
            )
        })
    }

    // XXX - We probably need to document/name this a little better as it's specifically for
    // history and NOT bookmarks...
    fn wipe_local_history(&self) -> Result<()> {
        self.with_conn(|conn| history::wipe_local(conn))
    }

    // Calls wipe_local_history but also updates the
    // sync metadata to only sync after most recent visit to prevent
    // further syncing of older data
    fn delete_everything_history(&self) -> Result<()> {
        self.with_conn(|conn| history::delete_everything(conn))
    }

    // XXX - This just calls wipe_local under the hood...
    // should probably have this go away?
    fn prune_destructively(&self) -> Result<()> {
        self.with_conn(|conn| history::prune_destructively(conn))
    }

    fn run_maintenance(&self) -> Result<()> {
        self.with_conn(|conn| storage::run_maintenance(conn))
    }

    fn query_autocomplete(&self, search: String, limit: i32) -> Result<Vec<SearchResult>> {
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

    fn accept_result(&self, search_string: String, url: Url) -> Result<()> {
        self.with_conn(|conn| matcher::accept_result(conn, &search_string, &url))
    }

    fn match_url(&self, query: String) -> Result<Option<Url>> {
        self.with_conn(|conn| matcher::match_url(conn, query))
    }

    fn bookmarks_get_tree(&self, item_guid: &Guid) -> Result<Option<BookmarkItem>> {
        self.with_conn(|conn| bookmarks::fetch::fetch_tree(conn, item_guid))
    }

    fn bookmarks_get_by_guid(
        &self,
        guid: &Guid,
        get_direct_children: bool,
    ) -> Result<Option<BookmarkItem>> {
        self.with_conn(|conn| {
            let bookmark = bookmarks::fetch::fetch_bookmark(conn, guid, get_direct_children)?;
            Ok(bookmark.map(BookmarkItem::from))
        })
    }

    fn bookmarks_get_all_with_url(&self, url: Url) -> Result<Vec<BookmarkItem>> {
        self.with_conn(|conn| {
            // XXX - We should return the exact type - ie, BookmarkData rather than BookmarkItem.
            Ok(bookmarks::fetch::fetch_bookmarks_by_url(conn, &url)?
                .into_iter()
                .map(|b| BookmarkItem::Bookmark { b })
                .collect())
        })
    }

    fn bookmarks_search(&self, query: String, limit: i32) -> Result<Vec<BookmarkItem>> {
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

    fn bookmarks_get_recent(&self, limit: i32) -> Result<Vec<BookmarkItem>> {
        self.with_conn(|conn| {
            // XXX - We should return the exact type - ie, BookmarkData rather than BookmarkItem.
            Ok(bookmarks::fetch::recent_bookmarks(conn, limit as u32)?
                .into_iter()
                .map(|b| BookmarkItem::Bookmark { b })
                .collect())
        })
    }

    fn bookmarks_delete(&self, id: Guid) -> Result<bool> {
        self.with_conn(|conn| bookmarks::delete_bookmark(conn, &id))
    }

    fn bookmarks_delete_everything(&self) -> Result<()> {
        self.with_conn(|conn| bookmarks::delete_everything(conn))
    }

    fn bookmarks_get_url_for_keyword(&self, keyword: String) -> Result<Option<Url>> {
        self.with_conn(|conn| bookmarks::bookmarks_get_url_for_keyword(conn, keyword.as_str()))
    }

    fn bookmarks_insert(&self, data: InsertableBookmarkItem) -> Result<Guid> {
        self.with_conn(|conn| bookmarks::insert_bookmark(conn, data))
    }

    fn bookmarks_update(&self, item: BookmarkUpdateInfo) -> Result<()> {
        self.with_conn(|conn| bookmarks::update_bookmark_from_info(conn, item))
    }
}

#[derive(Clone, PartialEq)]
pub struct HistoryVisitInfo {
    pub url: Url,
    pub title: Option<String>,
    pub timestamp: Timestamp,
    pub visit_type: VisitTransition,
    pub is_hidden: bool,
    pub preview_image_url: Option<Url>,
}
#[derive(Clone, PartialEq)]
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

// We define those types to cross the FFI
// a better approach would be to:
// - Rename the `Url` in the internal MatchReason to have a different name
//    This is because `uniffi` fails to parse the UDL if an enum variant
//    shadows a type, in this case, the wrapped type `Url`.
//    look at: https://github.com/mozilla/uniffi-rs/issues/1137
// - Fix the mismatch between the consumers and the rust layer with the Tags
//     variant in the internal MatchReason, the rust layer uses a
//     variant with associated data, the kotlin layers assumes a flat enum.
pub struct SearchResult {
    pub url: Url,
    pub title: String,
    pub frecency: i64,
    pub reasons: Vec<MatchReason>,
}

pub enum MatchReason {
    Keyword,
    Origin,
    UrlMatch,
    PreviousUse,
    Bookmark,
    Tags,
}

uniffi_macros::include_scaffolding!("places");
// Exists just to convince uniffi to generate `liftSequence*` helpers!
pub struct Dummy {
    md: Option<Vec<HistoryMetadata>>,
}
