/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::boxed::Box;
use std::{fmt, result, string};

use base64;
use failure::{Backtrace, Context, Fail};
#[cfg(feature = "browserid")]
use failure::SyncFailure;
#[cfg(feature = "browserid")]
use hawk;
use hex;
#[cfg(feature = "browserid")]
use openssl;
use reqwest;
use serde_json;

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
    #[fail(display = "Unknown OAuth State")]
    UnknownOAuthState,

    #[fail(display = "The client requested keys alongside the token but they were not included")]
    TokenWithoutKeys,

    #[fail(display = "Login state needs to be Married for the current operation")]
    NotMarried,

    #[fail(display = "No cached token for scope {}", _0)]
    NoCachedToken(&'static str),

    #[fail(display = "Unrecoverable server error")]
    UnrecoverableServerError,

    #[fail(display = "Invalid OAuth scope value {}", _0)]
    InvalidOAuthScopeValue(String),

    #[fail(display = "Empty names")]
    EmptyOAuthScopeNames,

    #[fail(display = "Key {} had wrong length, got {}, expected {}", _0, _1, _2)]
    BadKeyLength(&'static str, usize, usize),

    #[fail(display = "Cannot xor arrays with different lengths: {} and {}", _0, _1)]
    XorLengthMismatch(usize, usize),

    #[fail(display = "Audience URL without a host")]
    AudienceURLWithoutHost,

    #[fail(display = "Origin mismatch")]
    OriginMismatch,

    #[fail(display = "JWT signature validation failed")]
    JWTSignatureValidationFailed,

    #[fail(display = "ECDH key generation failed")]
    KeyGenerationFailed,

    #[fail(display = "Public key computation failed")]
    PublicKeyComputationFailed,

    #[fail(display = "Key agreement failed")]
    KeyAgreementFailed,

    #[fail(display = "Key import failed")]
    KeyImportFailed,

    #[fail(display = "AEAD open failure")]
    AEADOpenFailure,

    #[fail(display = "Random number generation failure")]
    RngFailure,

    #[fail(display = "HMAC verification failed")]
    HmacVerifyFail,

    #[fail(
        display = "Remote server error: '{}' '{}' '{}' '{}' '{}'", code, errno, error, message, info
    )]
    RemoteError {
        code: u64,
        errno: u64,
        error: String,
        message: String,
        info: String,
    },

    // Basically reimplement error_chain's foreign_links. (Ugh, this sucks)
    #[fail(display = "Hex decode error: {}", _0)]
    HexDecodeError(#[fail(cause)] hex::FromHexError),

    #[cfg(feature = "browserid")]
    #[fail(display = "OpenSSL error: {}", _0)]
    OpensslError(#[fail(cause)] openssl::error::ErrorStack),

    #[fail(display = "Base64 decode error: {}", _0)]
    Base64Decode(#[fail(cause)] base64::DecodeError),

    #[fail(display = "JSON error: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "UTF8 decode error: {}", _0)]
    UTF8DecodeError(#[fail(cause)] string::FromUtf8Error),

    #[fail(display = "Network error: {}", _0)]
    RequestError(#[fail(cause)] reqwest::Error),

    #[fail(display = "Malformed URL error: {}", _0)]
    MalformedUrl(#[fail(cause)] reqwest::UrlError),

    #[fail(display = "Header parsing error: {}", _0)]
    HeaderParseError(#[fail(cause)] reqwest::header::ToStrError),

    #[fail(display = "Malformed header error: {}", _0)]
    MalformedHeader(#[fail(cause)] reqwest::header::InvalidHeaderValue),

    #[cfg(feature = "browserid")]
    #[fail(display = "HAWK error: {}", _0)]
    HawkError(#[fail(cause)] SyncFailure<hawk::Error>),
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
    (HexDecodeError, ::hex::FromHexError),
    (Base64Decode, ::base64::DecodeError),
    (JsonError, ::serde_json::Error),
    (UTF8DecodeError, ::std::string::FromUtf8Error),
    (RequestError, ::reqwest::Error),
    (MalformedUrl, ::reqwest::UrlError),
    (HeaderParseError, ::reqwest::header::ToStrError),
    (MalformedHeader, ::reqwest::header::InvalidHeaderValue)
}

#[cfg(feature = "browserid")]
impl_from_error! {
    (OpensslError, ::openssl::error::ErrorStack)
}

// ::hawk::Error uses error_chain, and so it's not trivially compatible with failure.
// We have to box it inside a SyncError (which allows errors to be accessed from multiple
// threads at the same time, which failure requires for some reason...).
#[cfg(feature = "browserid")]
impl From<hawk::Error> for ErrorKind {
    #[inline]
    fn from(e: hawk::Error) -> ErrorKind {
        ErrorKind::HawkError(SyncFailure::new(e))
    }
}
#[cfg(feature = "browserid")]
impl From<hawk::Error> for Error {
    #[inline]
    fn from(e: hawk::Error) -> Error {
        ErrorKind::from(e).into()
    }
}
