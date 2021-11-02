/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::error::{Error, ErrorKind, InvalidPlaceInfo};
use crate::msg_types;
use crate::storage::history_metadata::{
    DocumentType, HistoryHighlight, HistoryHighlightWeights, HistoryMetadata,
    HistoryMetadataObservation,
};
use crate::{PlacesApi, PlacesDb};
use ffi_support::{
    implement_into_ffi_by_delegation, implement_into_ffi_by_protobuf, ConcurrentHandleMap,
    ErrorCode, ExternError, Handle, HandleError,
};
use std::sync::Arc;

lazy_static::lazy_static! {
    pub static ref APIS: ConcurrentHandleMap<Arc<PlacesApi>> = ConcurrentHandleMap::new();
    pub static ref CONNECTIONS: ConcurrentHandleMap<PlacesDb> = ConcurrentHandleMap::new();
}

fn parse_url(url: &str) -> crate::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

fn places_get_latest_history_metadata_for_url(
    handle: i64,
    url: String,
) -> Result<Option<HistoryMetadata>, ErrorWrapper> {
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            let url = parse_url(url.as_str())?;
            let metadata = crate::storage::history_metadata::get_latest_for_url(conn, &url)?;
            Ok(metadata)
        },
    )
}

fn places_get_history_metadata_between(
    handle: i64,
    start: i64,
    end: i64,
) -> Result<Vec<HistoryMetadata>, ErrorWrapper> {
    log::debug!("places_get_history_metadata_between");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            let between = crate::storage::history_metadata::get_between(conn, start, end)?;
            Ok(between)
        },
    )
}

fn places_get_history_metadata_since(
    handle: i64,
    start: i64,
) -> Result<Vec<HistoryMetadata>, ErrorWrapper> {
    log::debug!("places_get_history_metadata_since");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            let since = crate::storage::history_metadata::get_since(conn, start)?;
            Ok(since)
        },
    )
}

fn places_query_history_metadata(
    handle: i64,
    query: String,
    limit: i32,
) -> Result<Vec<HistoryMetadata>, ErrorWrapper> {
    log::debug!("places_get_history_metadata_since");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            let metadata = crate::storage::history_metadata::query(conn, query.as_str(), limit)?;
            Ok(metadata)
        },
    )
}

fn places_get_history_highlights(
    handle: i64,
    weights: HistoryHighlightWeights,
    limit: i32,
) -> Result<Vec<HistoryHighlight>, ErrorWrapper> {
    log::debug!("places_get_history_highlights");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            let highlights =
                crate::storage::history_metadata::get_highlights(conn, weights, limit)?;
            Ok(highlights)
        },
    )
}

fn places_note_history_metadata_observation(
    handle: i64,
    data: HistoryMetadataObservation,
) -> Result<(), ErrorWrapper> {
    log::debug!("places_note_history_metadata_observation");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            crate::storage::history_metadata::apply_metadata_observation(conn, data)?;
            Ok(())
        },
    )
}

fn places_metadata_delete_older_than(handle: i64, older_than: i64) -> Result<(), ErrorWrapper> {
    log::debug!("places_note_history_metadata_observation");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            crate::storage::history_metadata::delete_older_than(conn, older_than)?;
            Ok(())
        },
    )
}

fn places_metadata_delete(
    handle: i64,
    url: String,
    referrer_url: Option<String>,
    search_term: Option<String>,
) -> Result<(), ErrorWrapper> {
    log::debug!("places_metadata_delete_metadata");
    CONNECTIONS.get(
        Handle::from_u64(handle as u64)?,
        |conn| -> Result<_, ErrorWrapper> {
            crate::storage::history_metadata::delete_metadata(
                conn,
                url.as_str(),
                referrer_url.as_deref(),
                search_term.as_deref(),
            )?;
            Ok(())
        },
    )
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

/// This is very very hacky - we somehow need to ensure the same error hierarchy
/// exists for both hand-written FFI functions and those generated by uniffi,
/// and there doesn't seem to be a clean way of doing that. So our .udl defines
/// a single error type - ErrorWrapper::Wrapped(). The `String` message there
/// is, roughly, `format!("{}|{}", extern_error.code, extern_error.message)`.
/// There then exists code on the Swift and Kotlin side of the world which
/// unpacks this and returns the exact same error objects as if it was an
/// `ExternError` in the first place.
#[derive(Debug)]
pub enum ErrorWrapper {
    Wrapped(String),
}

impl ToString for ErrorWrapper {
    fn to_string(&self) -> String {
        match self {
            ErrorWrapper::Wrapped(e) => e.to_string(),
        }
    }
}

impl From<Error> for ErrorWrapper {
    fn from(e: Error) -> ErrorWrapper {
        ErrorWrapper::Wrapped(format!("{}|{}", get_error_number(&e), e.to_string()))
    }
}

impl From<HandleError> for ErrorWrapper {
    fn from(e: HandleError) -> ErrorWrapper {
        ErrorWrapper::Wrapped(format!("{}|{}", error_codes::UNEXPECTED, e.to_string()))
    }
}

impl From<Error> for ExternError {
    fn from(e: Error) -> ExternError {
        ExternError::new_error(get_code(&e), e.to_string())
    }
}

implement_into_ffi_by_protobuf!(msg_types::SearchResultList);
implement_into_ffi_by_protobuf!(msg_types::TopFrecentSiteInfos);
implement_into_ffi_by_protobuf!(msg_types::HistoryVisitInfos);
implement_into_ffi_by_protobuf!(msg_types::HistoryVisitInfosWithBound);
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
