/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This module implement the traits that make the FFI code easier to manage.

use crate::error::{Error, ErrorKind, InvalidPlaceInfo};
use ffi_support::{
    implement_into_ffi_by_delegation, ErrorCode, ExternError,
};

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
    match err.kind() {
        ErrorKind::InvalidPlaceInfo(info) => {
            log::error!("Invalid place info: {}", info);
            let code = match &info {
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
            };
            ErrorCode::new(code)
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
        ErrorKind::InterruptedError(_) => {
            // Can't unify with the above ... :(
            log::info!("Operation interrupted");
            ErrorCode::new(error_codes::DATABASE_INTERRUPTED)
        }
        ErrorKind::Corruption(e) => {
            log::info!("The store is corrupt: {}", e);
            ErrorCode::new(error_codes::DATABASE_CORRUPT)
        }
        ErrorKind::SyncAdapterError(e) => {
            use sync15::ErrorKind;
            match e.kind() {
                ErrorKind::StoreError(store_error) => {
                    // If it's a type-erased version of one of our errors, try
                    // and resolve it.
                    if let Some(places_err) = store_error.downcast_ref::<Error>() {
                        log::info!("Recursing to resolve places error");
                        get_code(places_err)
                    } else {
                        log::error!("Unexpected sync error: {:?}", err);
                        ErrorCode::new(error_codes::UNEXPECTED)
                    }
                }
                _ => {
                    // TODO: expose network errors...
                    log::error!("Unexpected sync error: {:?}", err);
                    ErrorCode::new(error_codes::UNEXPECTED)
                }
            }
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


/// Implements [`IntoFfi`] for the provided types (more than one may be passed in) implementing
/// `uniffi::ViaFfi` (UniFFI auto-generated serialization) by delegating to that implementation.
///
/// Note: for this to work, the crate it's called in must depend on `uniffi`.
///
/// Note: Each type passed in must implement or derive `uniffi::ViaFfi`.
#[macro_export]
macro_rules! implement_into_ffi_by_uniffi {
    ($($FFIType:ty),* $(,)*) => {$(
        unsafe impl ffi_support::IntoFfi for $FFIType where $FFIType: uniffi::ViaFfi {
            type Value = <Self as uniffi::ViaFfi>::FfiType;
            #[inline]
            fn ffi_default() -> Self::Value {
                Default::default()
            }

            #[inline]
            fn into_ffi_value(self) -> Self::Value {
                <Self as uniffi::ViaFfi>::lower(self)
            }
        }
    )*}
}


implement_into_ffi_by_uniffi!(crate::SearchResult);
implement_into_ffi_by_uniffi!(crate::TopFrecentSiteInfo);
implement_into_ffi_by_uniffi!(crate::HistoryVisitInfo);
implement_into_ffi_by_uniffi!(crate::types::HistoryVisitInfosWithBound);
implement_into_ffi_by_uniffi!(crate::types::BookmarkNode);
implement_into_ffi_by_delegation!(
    crate::storage::bookmarks::InternalBookmarkNode,
    crate::types::BookmarkNode
);
