/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::api::matcher::SearchResult;
use crate::db::PlacesInterruptHandle;
use crate::error::{Error, ErrorKind};
use crate::msg_types;
use ffi_support::{
    implement_into_ffi_by_delegation, implement_into_ffi_by_json, implement_into_ffi_by_pointer,
    implement_into_ffi_by_protobuf, ErrorCode, ExternError,
};

pub mod error_codes {
    // Note: 0 (success) and -1 (panic) are reserved by ffi_support

    /// An unexpected error occurred which likely cannot be meaningfully handled
    /// by the application.
    pub const UNEXPECTED: i32 = 1;

    /// The PlaceInfo we were given is invalid. (TODO: do we want to expose this as multiple
    /// error codes?)
    pub const INVALID_PLACE_INFO: i32 = 2;

    /// A URL was provided that we failed to parse
    pub const URL_PARSE_ERROR: i32 = 3;

    /// The requested operation failed because the database was busy
    /// performing operations on a separate connection to the same DB.
    pub const DATABASE_BUSY: i32 = 4;

    /// The requested operation failed because it was interrupted
    pub const DATABASE_INTERRUPTED: i32 = 5;

    /// The requested operation failed because the store is corrupt
    pub const DATABASE_CORRUPT: i32 = 6;
}

fn get_code(err: &Error) -> ErrorCode {
    match err.kind() {
        ErrorKind::InvalidPlaceInfo(info) => {
            log::error!("Invalid place info: {}", info);
            ErrorCode::new(error_codes::INVALID_PLACE_INFO)
        }
        ErrorKind::UrlParseError(e) => {
            log::error!("URL parse error: {}", e);
            ErrorCode::new(error_codes::URL_PARSE_ERROR)
        }
        // Can't pattern match on `err` without adding a dep on the sqlite3-sys crate,
        // so we just use a `if` guard.
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, msg))
            if err.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            log::error!("Database busy: {:?} {:?}", err, msg);
            ErrorCode::new(error_codes::DATABASE_BUSY)
        }
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::OperationInterrupted =>
        {
            log::info!("Operation interrupted");
            ErrorCode::new(error_codes::DATABASE_INTERRUPTED)
        }
        ErrorKind::InterruptedError => {
            // Can't unify with the above ... :(
            log::info!("Operation interrupted");
            ErrorCode::new(error_codes::DATABASE_INTERRUPTED)
        }
        ErrorKind::Corruption(e) => {
            log::info!("The store is corrupt: {}", e);
            ErrorCode::new(error_codes::DATABASE_CORRUPT)
        }

        err => {
            log::error!("Unexpected error: {:?}", err);
            ErrorCode::new(error_codes::UNEXPECTED)
        }
    }
}

impl From<Error> for ExternError {
    fn from(e: Error) -> ExternError {
        ExternError::new_error(get_code(&e), e.to_string())
    }
}

implement_into_ffi_by_json!(SearchResult);
implement_into_ffi_by_pointer!(PlacesInterruptHandle);
implement_into_ffi_by_protobuf!(msg_types::HistoryVisitInfos);
implement_into_ffi_by_protobuf!(msg_types::BookmarkNode);
implement_into_ffi_by_protobuf!(msg_types::BookmarkNodeList);
implement_into_ffi_by_delegation!(
    crate::storage::bookmarks::PublicNode,
    msg_types::BookmarkNode
);
