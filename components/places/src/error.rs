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

    #[error("Protobuf decode error: {0}")]
    ProtobufDecodeError(#[from] prost::DecodeError),

    #[error("UTF8 Error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("Database cannot be upgraded")]
    DatabaseUpgradeError,

    #[error("Database version {0} is not supported")]
    UnsupportedDatabaseVersion(i64),
}

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
        (ProtobufDecodeError, prost::DecodeError),
        (InterruptedError, Interrupted),
        (Utf8Error, std::str::Utf8Error),
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
