/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{self, panic, thread, ptr, process};
use std::os::raw::c_char;
use std::ffi::CString;
use rusqlite;

use logins_sql::{
    Result,
    Error,
    ErrorKind,
};

use sync15_adapter::{
    ErrorKind as Sync15ErrorKind
};

#[inline]
fn string_to_c_char(r_string: String) -> *mut c_char {
    CString::new(r_string).unwrap().into_raw()
}

// "Translate" in the next few functions refers to translating a rust Result
// type into a `(error, value)` tuple (well, sort of -- the `error` is taken as
// an out parameter and the value is all that's returned, but it's a conceptual
// tuple).

pub unsafe fn with_translated_result<F, T>(error: *mut ExternError, callback: F) -> *mut T
where F: FnOnce() -> Result<T> {
    match try_call_with_result(error, callback) {
        Some(v) => Box::into_raw(Box::new(v)),
        None => ptr::null_mut(),
    }
}

pub unsafe fn with_translated_void_result<F>(error: *mut ExternError, callback: F)
where F: FnOnce() -> Result<()> {
    let _: Option<()> = try_call_with_result(error, callback);
}

pub unsafe fn with_translated_value_result<F, T>(error: *mut ExternError, callback: F) -> T
where
    F: FnOnce() -> Result<T>,
    T: Default,
{
    try_call_with_result(error, callback).unwrap_or_default()
}

pub unsafe fn with_translated_string_result<F>(error: *mut ExternError, callback: F) -> *mut c_char
where F: FnOnce() -> Result<String> {
    if let Some(s) = try_call_with_result(error, callback) {
        string_to_c_char(s)
    } else {
        ptr::null_mut()
    }
}

pub unsafe fn with_translated_opt_string_result<F>(error: *mut ExternError, callback: F) -> *mut c_char
where F: FnOnce() -> Result<Option<String>> {
    if let Some(Some(s)) = try_call_with_result(error, callback) {
        string_to_c_char(s)
    } else {
        // This is either an error case, or callback returned None.
        ptr::null_mut()
    }
}

unsafe fn try_call_with_result<R, F>(out_error: *mut ExternError, callback: F) -> Option<R>
where F: FnOnce() -> Result<R> {
    // Ugh, using AssertUnwindSafe here is safe (in terms of memory safety),
    // but a lie -- this code may behave improperly in the case that we unwind.
    // That said, it's UB to unwind across the FFI boundary, and in practice
    // weird things happen if we do (we aren't caught on the other side).
    //
    // We should eventually figure out a better story here, possibly the
    // PasswordsEngine should get re-initialized if we hit this.
    let res: thread::Result<(ExternError, Option<R>)> =
        panic::catch_unwind(panic::AssertUnwindSafe(|| match callback() {
            Ok(v) => (ExternError::default(), Some(v)),
            Err(e) => (e.into(), None),
        }));
    match res {
        Ok((err, o)) => {
            if !out_error.is_null() {
                let eref = &mut *out_error;
                *eref = err;
            } else {
                error!("Fatal error: an error occurred but no error parameter was given {:?}", err);
                process::abort();
            }
            o
        }
        Err(e) => {
            if !out_error.is_null() {
                let eref = &mut *out_error;
                *eref = e.into();
            } else {
                let err: ExternError = e.into();
                error!("Fatal error: a panic occurred but no error parameter was given {:?}", err);
                process::abort();
            }
            None
        }
    }
}

/// C-compatible Error code. Negative codes are not expected to be handled by
/// the application, a code of zero indicates that no error occurred, and a
/// positive error code indicates an error that will likely need to be handled
/// by the application
#[repr(i32)]
#[derive(Clone, Copy, Debug)]
pub enum ExternErrorCode {

    /// An unexpected error occurred which likely cannot be meaningfully handled
    /// by the application.
    OtherError = -2,

    /// The rust code hit a `panic!` (or something equivalent, like `assert!`).
    UnexpectedPanic = -1,

    /// No error occcurred.
    NoError = 0,

    /// Indicates the FxA credentials are invalid, and should be refreshed.
    AuthInvalidError = 1,

    /// Returned from an `update()` call where the record ID did not exist.
    NoSuchRecord = 2,

    /// Returned from an `add()` call that was provided an ID, where the ID
    /// already existed.
    DuplicateGuid = 3,

    /// Attempted to insert or update a record so that it is invalid
    InvalidLogin = 4,

    /// Either the file is not a database, or it is not encrypted with the
    /// provided encryption key.
    InvalidKeyError = 5,

    /// A request to the sync server failed.
    NetworkError = 6,
}

/// Represents an error that occurred on the rust side. Many rust FFI functions take a
/// `*mut ExternError` as the last argument. This is an out parameter that indicates an
/// error that occurred during that function's execution (if any).
///
/// For functions that use this pattern, if the ExternError's message property is null, then no
/// error occurred. If the message is non-null then it contains a string description of the
/// error that occurred.
///
/// Important: This message is allocated on the heap and it is the consumer's responsibility to
/// free it!
///
/// While this pattern is not ergonomic in Rust, it offers two main benefits:
///
/// 1. It avoids defining a large number of `Result`-shaped types in the FFI consumer, as would
///    be required with something like an `struct ExternResult<T> { ok: *mut T, err:... }`
/// 2. It offers additional type safety over `struct ExternResult { ok: *mut c_void, err:... }`,
///    which helps avoid memory safety errors.
#[repr(C)]
#[derive(Debug)]
pub struct ExternError {

    /// A string message, primarially intended for debugging. This will be null
    /// in the case that no error occurred.
    pub message: *mut c_char,

    /// Error code.
    /// - A code of 0 indicates no error
    /// - A negative error code indicates an error which is not expected to be
    ///   handled by the application.
    pub code: ExternErrorCode,
}

impl Default for ExternError {
    fn default() -> ExternError {
        ExternError {
            message: ptr::null_mut(),
            code: ExternErrorCode::NoError,
        }
    }
}

fn get_code(err: &Error) -> ExternErrorCode {
    match err.kind() {
        ErrorKind::SyncAdapterError(e) => {
            error!("Sync error {:?}", e);
            match e.kind() {
                Sync15ErrorKind::TokenserverHttpError(401) => {
                    ExternErrorCode::AuthInvalidError
                },
                Sync15ErrorKind::RequestError(_) => {
                    ExternErrorCode::NetworkError
                }
                _ => ExternErrorCode::OtherError,
            }
        }
        ErrorKind::DuplicateGuid(id) => {
            error!("Guid already exists: {}", id);
            ExternErrorCode::DuplicateGuid
        }
        ErrorKind::NoSuchRecord(id) => {
            error!("No record exists with id {}", id);
            ExternErrorCode::NoSuchRecord
        }
        ErrorKind::InvalidLogin(desc) => {
            error!("Invalid login: {}", desc);
            ExternErrorCode::InvalidLogin
        }
        // We can't destructure `err` without bringing in the libsqlite3_sys crate
        // (and I'd really rather not) so we can't put this in the match.
        ErrorKind::SqlError(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::NotADatabase => {
            error!("Not a database / invalid key error");
            ExternErrorCode::InvalidKeyError
        }
        err => {
            error!("Unexpected error: {:?}", err);
            ExternErrorCode::OtherError
        }
    }
}

impl From<Error> for ExternError {
    fn from(e: Error) -> ExternError {
        let code = get_code(&e);
        let message = string_to_c_char(e.to_string());
        ExternError { message, code }
    }
}

// This is the `Err` of std::thread::Result, which is what
// `panic::catch_unwind` returns.
impl From<Box<std::any::Any + Send + 'static>> for ExternError {
    fn from(e: Box<std::any::Any + Send + 'static>) -> ExternError {
        // The documentation suggests that it will usually be a str or String.
        let message = if let Some(s) = e.downcast_ref::<&'static str>() {
            string_to_c_char(s.to_string())
        } else if let Some(s) = e.downcast_ref::<String>() {
            string_to_c_char(s.clone())
        } else {
            // Note that it's important that this be allocated on the heap,
            // since we'll free it later!
            string_to_c_char("Unknown panic!".into())
        };

        ExternError {
            code: ExternErrorCode::UnexpectedPanic,
            message,
        }
    }
}
