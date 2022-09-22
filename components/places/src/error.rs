/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::BookmarkType;
use error_support::{ErrorHandling, GetErrorHandling};
use interrupt_support::Interrupted;
use serde_json::Value as JsonValue;

// Result type used internally
pub type Result<T> = std::result::Result<T, PlacesInternalError>;
// Functions which are part of the public API should use this Result.
pub type ApiResult<T> = std::result::Result<T, PlacesError>;

// Errors we return via the public interface.
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
    // XXX - can we kill this?
    #[error("Invalid Bookmark: {0}")]
    InvalidBookmarkUpdate(String),

    /// Thrown when:
    /// - Attempting to insert a child under BookmarkRoot.Root,
    /// - Attempting to update any of the bookmark roots.
    /// - Attempting to delete any of the bookmark roots.
    #[error("CannotUpdateRoot error: {0}")]
    CannotUpdateRoot(String),

    // XX - Having `InternalError` is a smell and ideally it wouldn't exist
    // it exists to catch non-fatal unexpected errors
    /// Thrown when we catch an unexpected error
    /// that shouldn't be fatal
    #[error("Unexpected error: {0}")]
    InternalError(String),
}

// Internal Places error
#[derive(Debug, thiserror::Error)]
pub enum PlacesInternalError {
    #[error("Invalid place info: {0}")]
    InvalidPlaceInfo(#[from] InvalidPlaceInfo),

    #[error("The store is corrupt: {0}")]
    Corruption(#[from] Corruption),

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
    InvalidMetadataObservation(#[from] InvalidMetadataObservation),
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

// Define how our internal errors are handled and converted to external errors.
impl GetErrorHandling for PlacesInternalError {
    type ExternalError = PlacesError;

    // Return how to handle our internal errors
    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        // WARNING: The details inside the `PlacesError` we return should not
        // contain any personally identifying information.
        // However, because many of the string details come from the underlying
        // internal error, we operate on a best-effort basis, since we can't be
        // completely sure that our dependencies don't leak PII in their error
        // strings.  For example, `rusqlite::Error` could include data from a
        // user's database in their errors, which would then cause it to appear
        // in our `PlacesError::Unexpected` structs, log messages, etc.
        // But because we've never seen that in practice we are comfortable
        // forwarding that error message into ours without attempting to sanitize.
        match self {
            PlacesInternalError::InvalidPlaceInfo(info) => {
                let label = info.to_string();
                ErrorHandling::convert(match &info {
                    InvalidPlaceInfo::InvalidParent(..) | InvalidPlaceInfo::UrlTooLong => {
                        PlacesError::InvalidParent(label)
                    }
                    InvalidPlaceInfo::NoSuchGuid(..) => PlacesError::UnknownBookmarkItem(label),
                    InvalidPlaceInfo::IllegalChange(..) => {
                        PlacesError::InvalidBookmarkUpdate(label)
                    }
                    InvalidPlaceInfo::CannotUpdateRoot(..) => PlacesError::CannotUpdateRoot(label),
                    _ => PlacesError::UnexpectedPlacesException(label),
                })
                .report_error("places-invalid-place-info")
            }
            PlacesInternalError::UrlParseError(e) => {
                ErrorHandling::convert(PlacesError::UrlParseFailed(e.to_string()))
                    .report_error("places-url-parse-error")
            }
            // Can't pattern match on `err` without adding a dep on the sqlite3-sys crate,
            // so we just use a `if` guard.
            PlacesInternalError::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::DatabaseBusy =>
            {
                ErrorHandling::convert(PlacesError::PlacesConnectionBusy(self.to_string()))
                    .report_error("places-connection-busy")
            }
            PlacesInternalError::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::OperationInterrupted =>
            {
                ErrorHandling::convert(PlacesError::OperationInterrupted(self.to_string()))
                    .log_info()
            }
            PlacesInternalError::InterruptedError(err) => {
                // Can't unify with the above ... :(
                ErrorHandling::convert(PlacesError::OperationInterrupted(err.to_string()))
                    .log_info()
            }
            PlacesInternalError::Corruption(e) => {
                ErrorHandling::convert(PlacesError::BookmarksCorruption(e.to_string())).log_info()
            }
            PlacesInternalError::SyncAdapterError(e) => {
                match e {
                    sync15::Error::StoreError(store_error) => {
                        // If it's a type-erased version of one of our errors, try
                        // and resolve it.
                        if let Some(places_err) = store_error.downcast_ref::<PlacesInternalError>()
                        {
                            log::info!("Recursing to resolve places error");
                            places_err.get_error_handling()
                        } else {
                            ErrorHandling::convert(PlacesError::UnexpectedPlacesException(
                                self.to_string(),
                            ))
                            .report_error("places-unexpected-sync-error")
                        }
                    }
                    _ => ErrorHandling::convert(PlacesError::UnexpectedPlacesException(
                        self.to_string(),
                    ))
                    .report_error("places-unexpected-sync-error"),
                }
            }
            _ => ErrorHandling::convert(PlacesError::InternalError(self.to_string()))
                .report_error("places-unexpected-error"),
        }
    }
}
