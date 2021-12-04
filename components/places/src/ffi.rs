/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::api::places_api::places_api_new;
use crate::error::{Error, ErrorKind, InvalidPlaceInfo, PlacesError};
use crate::storage::history_metadata::{
    DocumentType, HistoryHighlight, HistoryHighlightWeights, HistoryMetadata,
    HistoryMetadataObservation,
};
use crate::storage::{history, history_metadata};
use crate::types::{HistoryVisitInfo, VisitTransitionSet};
use crate::ConnectionType;
use crate::VisitObservation;
use crate::VisitTransition;
use crate::{msg_types, HistoryVisitInfosWithBound};
use crate::{PlacesApi, PlacesDb};
use ffi_support::{
    implement_into_ffi_by_delegation, implement_into_ffi_by_protobuf, ConcurrentHandleMap,
    ErrorCode, ExternError,
};
use parking_lot::Mutex;
use std::sync::Arc;
use types::Timestamp;
use url::Url;

lazy_static::lazy_static! {
    pub static ref APIS: ConcurrentHandleMap<Arc<PlacesApi>> = ConcurrentHandleMap::new();
    pub static ref CONNECTIONS: ConcurrentHandleMap<PlacesDb> = ConcurrentHandleMap::new();
}

// All of our functions in this module use a `Result` type with the error we throw over
// the FFI.
type Result<T> = std::result::Result<T, PlacesError>;

impl UniffiCustomTypeWrapper for Url {
    type Wrapped = String;

    fn wrap(val: Self::Wrapped) -> uniffi::Result<url::Url> {
        Ok(Url::parse(val.as_str())?)
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

impl PlacesApi {
    fn new_connection(&self, conn_type: ConnectionType) -> Result<Arc<PlacesConnection>> {
        let db = self.open_connection(conn_type)?;
        let connection = PlacesConnection { db: Mutex::new(db) };
        Ok(Arc::new(connection))
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

    fn get_latest_history_metadata_for_url(&self, url: Url) -> Result<Option<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_latest_for_url(conn, &url))
    }

    fn get_history_metadata_between(&self, start: i64, end: i64) -> Result<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_between(conn, start, end))
    }

    fn get_history_metadata_since(&self, start: i64) -> Result<Vec<HistoryMetadata>> {
        self.with_conn(|conn| history_metadata::get_since(conn, start))
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

    fn metadata_delete_older_than(&self, older_than: i64) -> Result<()> {
        self.with_conn(|conn| history_metadata::delete_older_than(conn, older_than))
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
        match self.with_conn(|conn| history::apply_observation(conn, visit)) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn get_visited_urls_in_range(
        &self,
        start: i64,
        end: i64,
        include_remote: bool,
    ) -> Result<Vec<String>> {
        self.with_conn(|conn| {
            history::get_visited_urls(
                conn,
                // Probably should allow into()...
                types::Timestamp(start.max(0) as u64),
                types::Timestamp(end.max(0) as u64),
                include_remote,
            )
        })
    }

    fn get_visit_infos(
        &self,
        start_date: i64,
        end_date: i64,
        exclude_types: VisitTransitionSet,
    ) -> Result<Vec<HistoryVisitInfo>> {
        self.with_conn(|conn| {
            history::get_visit_infos(
                conn,
                types::Timestamp(start_date.max(0) as u64),
                types::Timestamp(end_date.max(0) as u64),
                exclude_types,
            )
        })
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

    fn get_visited(&self, urls: Vec<Url>) -> Result<Vec<bool>> {
        self.with_conn(|conn| history::get_visited(conn, urls))
    }

    fn delete_visits_for(&self, url: Url) -> Result<()> {
        self.with_conn(|conn| {
            let guid = match Url::parse(url.as_str()) {
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

    fn delete_visits_between(&self, start: i64, end: i64) -> Result<()> {
        self.with_conn(|conn| {
            history::delete_visits_between(
                conn,
                types::Timestamp(start.max(0) as u64),
                types::Timestamp(end.max(0) as u64),
            )
        })
    }

    fn delete_visit(&self, url: Url, timestamp: i64) -> Result<()> {
        self.with_conn(|conn| {
            match Url::parse(url.as_str()) {
                Ok(url) => {
                    history::delete_place_visit_at_time(
                        conn,
                        &url,
                        types::Timestamp(timestamp.max(0) as u64),
                    )?;
                }
                Err(e) => {
                    log::warn!("Invalid URL passed to places_delete_visit, {}", e);
                    history::delete_place_visit_at_time_by_href(
                        conn,
                        url.as_str(),
                        types::Timestamp(timestamp.max(0) as u64),
                    )?;
                }
            };
            Ok(())
        })
    }
}

pub mod error_codes {
    // Note: 0 (success) and -1 (panic) are reserved by ffi_support

    /// An unexpected error occurred which likely cannot be meaningfully handled
    /// by the application.
    pub const UNEXPECTED: i32 = 1;

    /// A URL was provided that we failed to parse
    pub const URL_PARSE_ERROR: i32 = 2;

    /// The requested operation failed because the database was busy
    /// performing operations on a separate connection to the same DB.
    pub const DATABASE_BUSY: i32 = 3;

    /// The requested operation failed because it was interrupted
    pub const DATABASE_INTERRUPTED: i32 = 4;

    /// The requested operation failed because the store is corrupt
    pub const DATABASE_CORRUPT: i32 = 5;

    // Skip a bunch of spaces to make it clear these are part of a group,
    // even as more and more errors get added. We're only exposing the
    // InvalidPlaceInfo items that can actually be triggered, the others
    // (if they happen accidentally) will come through as unexpected.

    /// `InvalidParent`: Attempt to add a child to a non-folder.
    pub const INVALID_PLACE_INFO_INVALID_PARENT: i32 = 64;

    /// `NoItem`: The GUID provided does not exist.
    pub const INVALID_PLACE_INFO_NO_ITEM: i32 = 64 + 1;

    /// `UrlTooLong`: The provided URL cannot be inserted, as it is over the
    /// maximum URL length.
    pub const INVALID_PLACE_INFO_URL_TOO_LONG: i32 = 64 + 2;

    /// `IllegalChange`: Attempt to change a property on a bookmark node that
    /// cannot have that property. E.g. trying to edit the URL of a folder,
    /// title of a separator, etc.
    pub const INVALID_PLACE_INFO_ILLEGAL_CHANGE: i32 = 64 + 3;

    /// `CannotUpdateRoot`: Attempt to modify a root in a way that is illegal, e.g. adding a child
    /// to root________, updating properties of a root, deleting a root, etc.
    pub const INVALID_PLACE_INFO_CANNOT_UPDATE_ROOT: i32 = 64 + 4;
}

fn get_code(err: &Error) -> ErrorCode {
    ErrorCode::new(get_error_number(err))
}

fn get_error_number(err: &Error) -> i32 {
    match err.kind() {
        ErrorKind::InvalidPlaceInfo(info) => {
            log::error!("Invalid place info: {}", info);
            match &info {
                InvalidPlaceInfo::InvalidParent(..) => {
                    error_codes::INVALID_PLACE_INFO_INVALID_PARENT
                }
                InvalidPlaceInfo::NoSuchGuid(..) => error_codes::INVALID_PLACE_INFO_NO_ITEM,
                InvalidPlaceInfo::UrlTooLong => error_codes::INVALID_PLACE_INFO_INVALID_PARENT,
                InvalidPlaceInfo::IllegalChange(..) => {
                    error_codes::INVALID_PLACE_INFO_ILLEGAL_CHANGE
                }
                InvalidPlaceInfo::CannotUpdateRoot(..) => {
                    error_codes::INVALID_PLACE_INFO_CANNOT_UPDATE_ROOT
                }
                _ => error_codes::UNEXPECTED,
            }
        }
        ErrorKind::UrlParseError(e) => {
            log::error!("URL parse error: {}", e);
            error_codes::URL_PARSE_ERROR
        }
        // Can't pattern match on `err` without adding a dep on the sqlite3-sys crate,
        // so we just use a `if` guard.
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, msg))
            if err.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            log::error!("Database busy: {:?} {:?}", err, msg);
            error_codes::DATABASE_BUSY
        }
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            log::info!("Operation interrupted");
            error_codes::DATABASE_INTERRUPTED
        }
        ErrorKind::InterruptedError(_) => {
            // Can't unify with the above ... :(
            log::info!("Operation interrupted");
            error_codes::DATABASE_INTERRUPTED
        }
        ErrorKind::Corruption(e) => {
            log::info!("The store is corrupt: {}", e);
            error_codes::DATABASE_CORRUPT
        }
        ErrorKind::SyncAdapterError(e) => {
            use sync15::ErrorKind;
            match e.kind() {
                ErrorKind::StoreError(store_error) => {
                    // If it's a type-erased version of one of our errors, try
                    // and resolve it.
                    if let Some(places_err) = store_error.downcast_ref::<Error>() {
                        log::info!("Recursing to resolve places error");
                        get_error_number(places_err)
                    } else {
                        log::error!("Unexpected sync error: {:?}", err);
                        error_codes::UNEXPECTED
                    }
                }
                _ => {
                    // TODO: expose network errors...
                    log::error!("Unexpected sync error: {:?}", err);
                    error_codes::UNEXPECTED
                }
            }
        }

        err => {
            log::error!("Unexpected error: {:?}", err);
            error_codes::UNEXPECTED
        }
    }
}

impl From<Error> for ExternError {
    fn from(e: Error) -> ExternError {
        ExternError::new_error(get_code(&e), e.to_string())
    }
}

implement_into_ffi_by_protobuf!(msg_types::SearchResultList);
implement_into_ffi_by_protobuf!(msg_types::TopFrecentSiteInfos);
implement_into_ffi_by_protobuf!(msg_types::BookmarkNode);
implement_into_ffi_by_protobuf!(msg_types::BookmarkNodeList);
implement_into_ffi_by_delegation!(
    crate::storage::bookmarks::PublicNode,
    msg_types::BookmarkNode
);

uniffi_macros::include_scaffolding!("places");
// Exists just to convince uniffi to generate `liftSequence*` helpers!
pub struct Dummy {
    md: Option<Vec<HistoryMetadata>>,
}
