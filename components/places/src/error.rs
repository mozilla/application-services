/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::BookmarkType;
use error_support::{ErrorHandling, GetErrorHandling};
use interrupt_support::Interrupted;

// reexport logging helpers.
pub use error_support::{debug, error, info, trace, warn};

// Result type used internally
pub type Result<T> = std::result::Result<T, Error>;
// Functions which are part of the public API should use this Result.
pub type ApiResult<T> = std::result::Result<T, PlacesApiError>;

// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum PlacesApiError {
    #[error("Unexpected error: {reason}")]
    UnexpectedPlacesException { reason: String },

    /// Thrown for invalid URLs
    ///
    /// This includes attempting to insert a URL greater than 65536 bytes
    /// (after punycoding and percent encoding).
    #[error("UrlParseFailed: {reason}")]
    UrlParseFailed { reason: String },

    #[error("PlacesConnectionBusy error: {reason}")]
    PlacesConnectionBusy { reason: String },

    #[error("Operation Interrupted: {reason}")]
    OperationInterrupted { reason: String },

    /// Thrown when providing a guid to a create or update function
    /// which does not refer to a known bookmark.
    #[error("Unknown bookmark: {reason}")]
    UnknownBookmarkItem { reason: String },

    /// Attempt to create/update/delete a bookmark item in an illegal way.
    ///
    /// Some examples:
    ///  - Attempting to change the URL of a bookmark folder
    ///  - Referring to a non-folder as the parentGUID parameter to a create or update
    ///  - Attempting to insert a child under BookmarkRoot.Root,
    #[error("Invalid bookmark operation: {reason}")]
    InvalidBookmarkOperation { reason: String },
}

/// Error enum used internally
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid place info: {0}")]
    InvalidPlaceInfo(#[from] InvalidPlaceInfo),

    #[error("The store is corrupt: {0}")]
    Corruption(#[from] Corruption),

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

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = PlacesApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Error::InvalidPlaceInfo(info) => {
                let label = info.to_string();
                ErrorHandling::convert(match &info {
                    InvalidPlaceInfo::InvalidParent(..) => {
                        PlacesApiError::InvalidBookmarkOperation { reason: label }
                    }
                    InvalidPlaceInfo::UrlTooLong => {
                        PlacesApiError::UrlParseFailed { reason: label }
                    }
                    InvalidPlaceInfo::NoSuchGuid(..) => {
                        PlacesApiError::UnknownBookmarkItem { reason: label }
                    }
                    InvalidPlaceInfo::IllegalChange(..) => {
                        PlacesApiError::InvalidBookmarkOperation { reason: label }
                    }
                    InvalidPlaceInfo::CannotUpdateRoot(..) => {
                        PlacesApiError::InvalidBookmarkOperation { reason: label }
                    }
                    _ => PlacesApiError::UnexpectedPlacesException { reason: label },
                })
                .report_error("places-invalid-place-info")
            }
            Error::UrlParseError(e) => {
                // This is a known issue with invalid URLs coming from Fenix. Let's just log a
                // warning for this one. See #5235 for more details.
                ErrorHandling::convert(PlacesApiError::UrlParseFailed {
                    reason: e.to_string(),
                })
                .log_warning()
            }
            Error::SqlError(rusqlite::Error::SqliteFailure(err, _)) => match err.code {
                rusqlite::ErrorCode::DatabaseBusy => {
                    ErrorHandling::convert(PlacesApiError::PlacesConnectionBusy {
                        reason: self.to_string(),
                    })
                    .log_warning()
                }
                rusqlite::ErrorCode::OperationInterrupted => {
                    ErrorHandling::convert(PlacesApiError::OperationInterrupted {
                        reason: self.to_string(),
                    })
                    .log_info()
                }
                rusqlite::ErrorCode::DatabaseCorrupt => {
                    ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                        reason: self.to_string(),
                    })
                    .report_error("places-db-corrupt")
                }
                rusqlite::ErrorCode::DiskFull => {
                    ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                        reason: self.to_string(),
                    })
                    .report_error("places-db-disk-full")
                }
                _ => ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                    reason: self.to_string(),
                })
                .report_error("places-unexpected"),
            },
            Error::InterruptedError(err) => {
                // Can't unify with the above ... :(
                ErrorHandling::convert(PlacesApiError::OperationInterrupted {
                    reason: err.to_string(),
                })
                .log_info()
            }
            Error::Corruption(e) => {
                ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                    reason: e.to_string(),
                })
                .report_error("places-bookmarks-corruption")
            }
            Error::InvalidMetadataObservation(InvalidMetadataObservation::ViewTimeTooLong) => {
                ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                    reason: self.to_string(),
                })
                .log_warning()
            }
            _ => ErrorHandling::convert(PlacesApiError::UnexpectedPlacesException {
                reason: self.to_string(),
            })
            .report_error("places-unexpected-error"),
        }
    }
}
