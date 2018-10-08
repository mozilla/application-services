/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![cfg(feature = "ffi")]

// This module implement the traits that make the FFI code easier to manage.

use ffi_support::{ErrorCode, ExternError};
use api::matcher::SearchResult;
use db::PlacesDb;
use error::{Error, ErrorKind};

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
}

fn get_code(err: &Error) -> ErrorCode {
    match err.kind() {
        ErrorKind::InvalidPlaceInfo(info) => {
            error!("Invalid place info: {}", info);
            ErrorCode::new(error_codes::INVALID_PLACE_INFO)
        }
        ErrorKind::UrlParseError(e) => {
            error!("URL parse error: {}", e);
            ErrorCode::new(error_codes::URL_PARSE_ERROR)
        }
        err => {
            error!("Unexpected error: {:?}", err);
            ErrorCode::new(error_codes::UNEXPECTED)
        }
    }
}

impl From<Error> for ExternError {
    fn from(e: Error) -> ExternError {
        ExternError::new_error(get_code(&e), e.to_string())
    }
}

implement_into_ffi_by_pointer!(PlacesDb);
implement_into_ffi_by_json!(SearchResult);
