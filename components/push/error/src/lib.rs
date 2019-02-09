/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;
use std::result;

use rusqlite;

use failure::{Backtrace, Context, Fail};

pub type Result<T> = result::Result<T, Error>;

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

    pub fn internal(msg: &str) -> Self {
        ErrorKind::InternalError(msg.to_owned()).into()
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

impl From<openssl::error::ErrorStack> for Error {
    #[inline]
    fn from(inner: openssl::error::ErrorStack) -> Error {
        Error(Box::new(Context::new(ErrorKind::OpenSSLError(format!(
            "{:?}",
            inner
        )))))
    }
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
    (StorageSqlError, rusqlite::Error)
}

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "General Error: {:?}", _0)]
    GeneralError(String),

    #[fail(display = "Internal Error: {:?}", _0)]
    InternalError(String),

    #[fail(display = "OpenSSL Error: {:?}", _0)]
    OpenSSLError(String),

    #[fail(display = "Communication Error: {:?}", _0)]
    CommunicationError(String),

    #[fail(display = "Channel already registered.")]
    AlreadyRegisteredError,

    #[fail(display = "Storage Error: {:?}", _0)]
    StorageError(String),

    #[fail(display = "Error executing SQL: {}", _0)]
    StorageSqlError(#[fail(cause)] rusqlite::Error),
}
