/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use failure::{Backtrace, Context, Fail};
use std::boxed::Box;
use std::{self, fmt};

pub type Result<T> = std::result::Result<T, Error>;

// Backported part of the (someday real) failure 1.x API, basically equivalent
// to error_chain's `bail!` (We don't call it that because `failure` has a
// `bail` macro with different semantics)
macro_rules! throw {
    ($e:expr) => {
        return Err(::std::convert::Into::into($e));
    };
}

#[derive(Debug)]
pub struct Error(Box<Context<ErrorKind>>);

impl Fail for Error {
    #[inline]
    fn cause(&self) -> Option<&dyn Fail> {
        self.0.cause()
    }

    #[inline]
    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Invalid login: {}", _0)]
    InvalidLogin(InvalidLogin),

    #[fail(
        display = "The `sync_status` column in DB has an illegal value: {}",
        _0
    )]
    BadSyncStatus(u8),

    #[fail(display = "A duplicate GUID is present: {:?}", _0)]
    DuplicateGuid(String),

    #[fail(
        display = "No record with guid exists (when one was required): {:?}",
        _0
    )]
    NoSuchRecord(String),

    #[fail(display = "Error synchronizing: {}", _0)]
    SyncAdapterError(#[fail(cause)] sync15::Error),

    #[fail(display = "Error parsing JSON data: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "Error executing SQL: {}", _0)]
    SqlError(#[fail(cause)] rusqlite::Error),

    #[fail(display = "Error parsing URL: {}", _0)]
    UrlParseError(#[fail(cause)] url::ParseError),

    #[fail(display = "{}", _0)]
    Interrupted(#[fail(cause)] interrupt::Interrupted),
}

macro_rules! impl_from_error {
    ($(($variant:ident, $type:ty)),+ $(,)?) => ($(
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
    (InvalidLogin, InvalidLogin),
    (Interrupted, interrupt::Interrupted),
}

#[derive(Debug, Fail)]
pub enum InvalidLogin {
    #[fail(display = "Hostname is empty")]
    EmptyHostname,
    #[fail(display = "Password is empty")]
    EmptyPassword,
    #[fail(display = "Both `formSubmitUrl` and `httpRealm` are present")]
    BothTargets,
    #[fail(display = "Neither `formSubmitUrl` and `httpRealm` are present")]
    NoTarget,
}
