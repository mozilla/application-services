/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::BookmarkType;
use interrupt_support::Interrupted;
use serde_json::Value as JsonValue;
// Note: If you add new error types that should be returned to consumers on the other side of the
// FFI, update `get_code` in `ffi.rs`
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("Invalid place info: {0}")]
    InvalidPlaceInfo(InvalidPlaceInfo),

    #[error("The store is corrupt: {0}")]
    Corruption(Corruption),

    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    #[error("Error merging: {0}")]
    MergeError(#[from] dogear::Error),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("A connection of this type is already open")]
    ConnectionAlreadyOpen,

    #[error("An invalid connection type was specified")]
    InvalidConnectionType,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Operation interrupted")]
    InterruptedError(#[from] Interrupted),

    #[error("Component shutdown")]
    ShutdownError(#[from] shutdown::ShutdownError),

    #[error("Tried to close connection on wrong PlacesApi instance")]
    WrongApiForClose,

    #[error("Incoming bookmark missing type")]
    MissingBookmarkKind,

    #[error("Incoming bookmark has unsupported type {0}")]
    UnsupportedIncomingBookmarkType(JsonValue),

    #[error("Synced bookmark has unsupported kind {0}")]
    UnsupportedSyncedBookmarkKind(u8),

    #[error("Synced bookmark has unsupported validity {0}")]
    UnsupportedSyncedBookmarkValidity(u8),

    // This will happen if you provide something absurd like
    // "/" or "" as your database path. For more subtley broken paths,
    // we'll likely return an IoError.
    #[error("Illegal database path: {0:?}")]
    IllegalDatabasePath(std::path::PathBuf),

    #[error("UTF8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    // This error is saying an old Fennec or iOS version isn't supported - it's never used for
    // our specific version.
    #[error("Can not import from database version {0}")]
    UnsupportedDatabaseVersion(i64),

    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),

    #[error("Invalid metadata observation: {0}")]
    InvalidMetadataObservation(InvalidMetadataObservation),
}

// This defines the `Error` and `Result` types exported by this module.
// These errors do not make it across the FFI, so can be considered "private" to the
// Rust side of the world.
error_support::define_error! {
    ErrorKind {
        (SyncAdapterError, sync15::Error),
        (JsonError, serde_json::Error),
        (UrlParseError, url::ParseError),
        (SqlError, rusqlite::Error),
        (InvalidPlaceInfo, InvalidPlaceInfo),
        (Corruption, Corruption),
        (IoError, std::io::Error),
        (MergeError, dogear::Error),
        (InterruptedError, Interrupted),
        (ShutdownError, shutdown::ShutdownError),
        (Utf8Error, std::str::Utf8Error),
        (OpenDatabaseError, sql_support::open_database::Error),
        (InvalidMetadataObservation, InvalidMetadataObservation),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidPlaceInfo {
    #[error("No url specified")]
    NoUrl,
    #[error("Invalid guid")]
    InvalidGuid,
    #[error("Invalid parent: {0}")]
    InvalidParent(String),
    #[error("Invalid child guid")]
    InvalidChildGuid,

    // NoSuchGuid is used for guids, which aren't considered private information,
    // so it's fine if this error, including the guid, is in the logs.
    #[error("No such item: {0}")]
    NoSuchGuid(String),

    // NoSuchUrl is used for URLs, which are private information, so the URL
    // itself is not included in the error.
    #[error("No such url")]
    NoSuchUrl,

    #[error("Can't update a bookmark of type {0} with one of type {1}")]
    MismatchedBookmarkType(u8, u8),

    // Only returned when attempting to insert a bookmark --
    // for history we just ignore it.
    #[error("URL too long")]
    UrlTooLong,

    // Like Urls, a tag is considered private info, so the value isn't in the error.
    #[error("The tag value is invalid")]
    InvalidTag,
    #[error("Cannot change the '{0}' property of a bookmark of type {1:?}")]
    IllegalChange(&'static str, BookmarkType),

    #[error("Cannot update the bookmark root {0:?}")]
    CannotUpdateRoot(BookmarkRootGuid),
}

// Error types used when we can't continue due to corruption.
// Note that this is currently only for "logical" corruption. Should we
// consider mapping sqlite error codes which mean a lower-level of corruption
// into an enum value here?
#[derive(Debug, thiserror::Error)]
pub enum Corruption {
    #[error("Bookmark '{0}' has a parent of '{1}' which does not exist")]
    NoParent(String, String),

    #[error("The local roots are invalid")]
    InvalidLocalRoots,

    #[error("The synced roots are invalid")]
    InvalidSyncedRoots,

    #[error("Bookmark '{0}' has no parent but is not the bookmarks root")]
    NonRootWithoutParent(String),
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidMetadataObservation {
    #[error("Observed view time is invalid (too long)")]
    ViewTimeTooLong,
}

// This is the error object thrown over the FFI.
#[derive(Debug, thiserror::Error)]
pub enum PlacesError {
    #[error("Unexpected error: {0}")]
    UnexpectedPlacesException(String),

    #[error("UrlParseFailed: {0}")]
    UrlParseFailed(String),

    #[error("JsonParseFailed: {0}")]
    JsonParseFailed(String),

    #[error("PlacesConnectionBusy error: {0}")]
    PlacesConnectionBusy(String),

    #[error("Operation Interrupted: {0}")]
    OperationInterrupted(String),

    /// Error indicating bookmarks corruption. If this occurs, we
    /// would appreciate reports.
    ///
    /// Eventually it should be fixed up, when detected as part of
    /// `runMaintenance`.
    #[error("BookmarksCorruption error: {0}")]
    BookmarksCorruption(String),

    /// Thrown when providing a guid referring to a non-folder as the
    /// parentGUID parameter to a create or update
    #[error("Invalid Parent: {0}")]
    InvalidParent(String),

    /// Thrown when providing a guid to a create or update function
    /// which does not refer to a known bookmark.
    #[error("Unknown bookmark: {0}")]
    UnknownBookmarkItem(String),

    /// Thrown when attempting to insert a URL greater than 65536 bytes
    /// (after punycoding and percent encoding).
    ///
    /// Attempting to truncate the URL is difficult and subtle, and
    /// is guaranteed to result in a URL different from the one the
    /// user attempted to bookmark, and so an error is thrown instead.
    #[error("URL too long: {0}")]
    UrlTooLong(String),

    /// Thrown when attempting to update a bookmark item in an illegal
    /// way. For example, attempting to change the URL of a bookmark
    /// folder, or update the title of a separator, etc.
    #[error("Invalid Bookmark: {0}")]
    InvalidBookmarkUpdate(String),

    /// Thrown when:
    /// - Attempting to insert a child under BookmarkRoot.Root,
    /// - Attempting to update any of the bookmark roots.
    /// - Attempting to delete any of the bookmark roots.
    #[error("CannotUpdateRoot error: {0}")]
    CannotUpdateRoot(String),

    #[error("Unexpected error: {0}")]
    InternalPanic(String),
}

// A port of the error conversion stuff that was in ffi.rs - it turns our
// "internal" errors into "public" ones.
fn make_places_error(error: &Error) -> PlacesError {
    let label = error.to_string();
    let kind = error.kind();
    match kind {
        ErrorKind::InvalidPlaceInfo(info) => {
            log::error!("Invalid place info: {}", info);
            let label = info.to_string();
            match &info {
                InvalidPlaceInfo::InvalidParent(..) | InvalidPlaceInfo::UrlTooLong => {
                    PlacesError::InvalidParent(label)
                }
                InvalidPlaceInfo::NoSuchGuid(..) => PlacesError::UnknownBookmarkItem(label),
                InvalidPlaceInfo::IllegalChange(..) => PlacesError::InvalidBookmarkUpdate(label),
                InvalidPlaceInfo::CannotUpdateRoot(..) => PlacesError::CannotUpdateRoot(label),
                _ => PlacesError::UnexpectedPlacesException(label),
            }
        }
        ErrorKind::UrlParseError(e) => {
            log::error!("URL parse error: {}", e);
            PlacesError::UrlParseFailed(e.to_string())
        }
        // Can't pattern match on `err` without adding a dep on the sqlite3-sys crate,
        // so we just use a `if` guard.
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, msg))
            if err.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            log::error!("Database busy: {:?} {:?}", err, msg);
            PlacesError::PlacesConnectionBusy(label)
        }
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            log::info!("Operation interrupted");
            PlacesError::OperationInterrupted(label)
        }
        ErrorKind::InterruptedError(err) => {
            // Can't unify with the above ... :(
            log::info!("Operation interrupted");
            PlacesError::OperationInterrupted(err.to_string())
        }
        ErrorKind::Corruption(e) => {
            log::info!("The store is corrupt: {}", e);
            PlacesError::BookmarksCorruption(e.to_string())
        }
        ErrorKind::SyncAdapterError(e) => {
            use sync15::ErrorKind;
            match e.kind() {
                ErrorKind::StoreError(store_error) => {
                    // If it's a type-erased version of one of our errors, try
                    // and resolve it.
                    if let Some(places_err) = store_error.downcast_ref::<Error>() {
                        log::info!("Recursing to resolve places error");
                        make_places_error(places_err)
                    } else {
                        log::error!("Unexpected sync error: {:?}", label);
                        PlacesError::UnexpectedPlacesException(label)
                    }
                }
                _ => {
                    log::error!("Unexpected sync error: {:?}", label);
                    PlacesError::UnexpectedPlacesException(label)
                }
            }
        }

        err => {
            log::error!("Unexpected error: {:?}", err);
            PlacesError::InternalPanic(label)
        }
    }
}

impl From<Error> for PlacesError {
    fn from(e: Error) -> PlacesError {
        make_places_error(&e)
    }
}

impl From<serde_json::Error> for PlacesError {
    fn from(e: serde_json::Error) -> PlacesError {
        PlacesError::JsonParseFailed(format!("{}", e))
    }
}
