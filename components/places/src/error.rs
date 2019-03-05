/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXX - more copy-pasta from logins.

use failure::{Backtrace, Context, Fail};
use std::boxed::Box;
use std::{self, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error(Box<Context<ErrorKind>>);

impl Fail for Error {
    #[inline]
    fn cause(&self) -> Option<&Fail> {
        self.0.cause()
    }

    #[inline]
    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl Error {
    #[inline]
    pub fn kind(&self) -> &ErrorKind {
        &*self.0.get_context()
    }
}

impl From<ErrorKind> for Error {
    #[inline]
    fn from(kind: ErrorKind) -> Error {
        Error(Box::new(Context::new(kind)))
    }
}

impl From<Context<ErrorKind>> for Error {
    #[inline]
    fn from(inner: Context<ErrorKind>) -> Error {
        Error(Box::new(inner))
    }
}

// Note: If you add new error types that should be returned to consumers on the other side of the
// FFI, update `get_code` in `ffi.rs`
#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Invalid place info: {}", _0)]
    InvalidPlaceInfo(InvalidPlaceInfo),

    #[fail(display = "The store is corrupt: {}", _0)]
    Corruption(Corruption),

    #[fail(display = "Error synchronizing: {}", _0)]
    SyncAdapterError(#[fail(cause)] sync15::Error),

    #[fail(display = "Error parsing JSON data: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "Error executing SQL: {}", _0)]
    SqlError(#[fail(cause)] rusqlite::Error),

    #[fail(display = "Error parsing URL: {}", _0)]
    UrlParseError(#[fail(cause)] url::ParseError),

    // Maybe we should try to fabricate a rusqlite::Error that looks like the
    // interrupted error?
    #[fail(display = "Operation interrupted")]
    InterruptedError,
}

macro_rules! impl_from_error {
    ($(($variant:ident, $type:ty)),+) => ($(
        impl From<$type> for ErrorKind {
            #[inline]
            fn from(e: $type) -> ErrorKind {
                ErrorKind::$variant(e)
            }
        }

        impl From<$type> for Error {
            #[inline]
            fn from(e: $type) -> Error {
                ErrorKind::from(e).into()
            }
        }
    )*);
}

impl_from_error! {
    (SyncAdapterError, sync15::Error),
    (JsonError, serde_json::Error),
    (UrlParseError, url::ParseError),
    (SqlError, rusqlite::Error),
    (InvalidPlaceInfo, InvalidPlaceInfo),
    (Corruption, Corruption)
}

#[derive(Debug, Fail)]
pub enum InvalidPlaceInfo {
    #[fail(display = "No url specified")]
    NoUrl,
    #[fail(display = "Invalid guid")]
    InvalidGuid,
    #[fail(display = "Invalid parent: {}", _0)]
    InvalidParent(String),

    #[fail(display = "No such item: {}", _0)]
    NoItem(String),

    #[fail(
        display = "Can't update a bookmark of type {} with one of type {}",
        _0, _1
    )]
    MismatchedBookmarkType(u8, u8),

    // Only returned when attempting to insert a bookmark --
    // for history we just ignore it.
    #[fail(display = "URL too long")]
    UrlTooLong,
}

// Error types used when we can't continue due to corruption.
// Note that this is currently only for "logical" corruption. Should we
// consider mapping sqlite error codes which mean a lower-level of corruption
// into an enum value here?
#[derive(Debug, Fail)]
pub enum Corruption {
    #[fail(
        display = "Bookmark '{}' has a parent of '{}' which does not exist",
        _0, _1
    )]
    NoParent(String, String),
}
