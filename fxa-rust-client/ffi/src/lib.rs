/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate fxa_client;
extern crate libc;

mod ctypes;
mod util;
use std::ffi::CString;
use std::ptr;

use ctypes::*;
use fxa_client::errors::Error as InternalError;
use fxa_client::errors::ErrorKind::*;
use fxa_client::{Config, FirefoxAccount, WebChannelResponse};
use libc::c_char;
use util::*;

#[repr(C)]
#[derive(Debug)]
pub enum ErrorCode {
    NoError = 0,
    Other = 1,
    AuthenticationError = 2,
    InternalPanic = 3,
}

/// An error struct containing an error code and a description string. Callers
/// should create values of this type locally and pass pointers to them in as
/// the last argument of functions which may fail.
///
/// In the case that an error occurs, callers are responsible for freeing the
/// string stored in `message` using fxa_str_free.
#[repr(C)]
#[derive(Debug)]
pub struct ExternError {
    pub code: ErrorCode,
    pub message: *mut c_char,
}

impl Default for ExternError {
    fn default() -> ExternError {
        ExternError {
            code: ErrorCode::NoError,
            message: ptr::null_mut(),
        }
    }
}

impl From<InternalError> for ExternError {
    fn from(err: InternalError) -> ExternError {
        match err {
            InternalError(RemoteError(401, ..), ..)
            | InternalError(NotMarried, ..)
            | InternalError(NeededTokenNotFound, ..) => ExternError {
                code: ErrorCode::AuthenticationError,
                message: string_to_c_char(err.to_string()),
            },
            err => ExternError {
                code: ErrorCode::Other,
                message: string_to_c_char(err.to_string()),
            },
        }
    }
}

// This is the `Err` of std::thread::Result, which is what
// `std::panic::catch_unwind` returns.
impl From<Box<std::any::Any + Send + 'static>> for ExternError {
    fn from(e: Box<std::any::Any + Send + 'static>) -> ExternError {
        // The documentation suggests that it will usually be a str or String.
        let message = if let Some(s) = e.downcast_ref::<&'static str>() {
            string_to_c_char(*s)
        } else if let Some(s) = e.downcast_ref::<String>() {
            string_to_c_char(s.clone())
        } else {
            // Note that it's important that this be allocated on the heap,
            // since we'll free it later!
            string_to_c_char("Unknown panic!")
        };

        ExternError {
            code: ErrorCode::InternalPanic,
            message,
        }
    }
}

/// Call a function returning Result<T, E> inside catch_unwind, writing any error
/// or panic into ExternError.
///
/// In the case the call returns an error, information about this will be
/// written into the ExternError, and a null pointer will be returned.
///
/// In the case that the call succeeds, then the ExternError will have
/// `code == ErrorCode::NoError` and `message == ptr::null_mut()`.
///
/// Note that we allow out_error to be null (it's not like we can panic if it's
/// not...), but *highly* discourage doing so. We will log error information to
/// stderr in the case that something goes wrong and you fail to provide an
/// error output.
///
/// Note: it's undefined behavior (e.g. very bad) to panic across the FFI
/// boundary, so it's important that we wrap calls that may fail in catch_unwind
/// like this.
unsafe fn call_with_result<R, F>(out_error: *mut ExternError, callback: F) -> *mut R
where
    F: std::panic::UnwindSafe + FnOnce() -> Result<R, InternalError>,
{
    try_call_with_result(out_error, callback)
        .map(|v| Box::into_raw(Box::new(v)))
        .unwrap_or(ptr::null_mut())
}

/// A version of call_with_result for the cases when `R` is a type you'd like
/// to return directly to C. For example, a `*mut c_char`, or a `#[repr(C)]`
/// struct.
///
/// This requires you provide a default value to return in the error case.
unsafe fn call_with_result_by_value<R, F>(out_error: *mut ExternError, default: R, callback: F) -> R
where
    F: std::panic::UnwindSafe + FnOnce() -> Result<R, InternalError>,
{
    try_call_with_result(out_error, callback).unwrap_or(default)
}

/// Helper for the fairly common case where we want to return a string to C.
unsafe fn call_with_string_result<R, F>(out_error: *mut ExternError, callback: F) -> *mut c_char
where
    F: std::panic::UnwindSafe + FnOnce() -> Result<R, InternalError>,
    R: Into<String>,
{
    call_with_result_by_value(out_error, ptr::null_mut(), || {
        callback().map(string_to_c_char)
    })
}

/// Common code between call_with_result and call_with_result_by_value.
unsafe fn try_call_with_result<R, F>(out_error: *mut ExternError, callback: F) -> Option<R>
where
    F: std::panic::UnwindSafe + FnOnce() -> Result<R, InternalError>,
{
    let res: std::thread::Result<(ExternError, Option<R>)> =
        std::panic::catch_unwind(|| match callback() {
            Ok(v) => (ExternError::default(), Some(v)),
            Err(e) => (e.into(), None),
        });
    match res {
        Ok((err, o)) => {
            if !out_error.is_null() {
                let eref = &mut *out_error;
                *eref = err;
            } else {
                eprintln!(
                    "Warning: an error occurred but no error parameter was given: {:?}",
                    err
                );
            }
            o
        }
        Err(e) => {
            if !out_error.is_null() {
                let eref = &mut *out_error;
                *eref = e.into();
            } else {
                let err: ExternError = e.into();
                eprintln!(
                    "Warning: a panic occurred but no error parameter was given: {:?}",
                    err
                );
            }
            None
        }
    }
}

/// Convenience function over [fxa_get_custom_config] that provides a pointer to a [Config] that
/// points to the production FxA servers.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_release_config(err: *mut ExternError) -> *mut Config {
    call_with_result(err, Config::release)
}

/// Creates a [Config] by making a request to `<content_base>/.well-known/fxa-client-configuration`
/// and parsing the newly fetched configuration object.
///
/// Note: `content_base` shall not have a trailing slash.
///
/// Returns a [Result<Config>](fxa_client::Config) as an [ExternResult].
///
/// # Safety
///
/// Please note that most methods taking a [Config] as argument will take ownership of it and
/// therefore the callers shall **not** free the [Config] afterwards.
///
/// A destructor [fxa_config_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_custom_config(
    content_base: *const c_char,
    err: *mut ExternError,
) -> *mut Config {
    call_with_result(err, || Config::import_from(c_char_to_string(content_base)))
}

/// Creates a [FirefoxAccount] from credentials obtained with the onepw FxA login flow.
///
/// This is typically used by the legacy Sync clients: new clients mainly use OAuth flows and
/// therefore should use `fxa_new`.
///
/// Note: This takes ownership of `Config`.
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_from_credentials(
    config: *mut Config,
    client_id: *const c_char,
    redirect_uri: *const c_char,
    json: *const c_char,
    err: *mut ExternError,
) -> *mut FirefoxAccount {
    call_with_result(err, || {
        assert!(!config.is_null());
        let config = Box::from_raw(config);
        let json = c_char_to_string(json);
        let client_id = c_char_to_string(client_id);
        let redirect_uri = c_char_to_string(redirect_uri);
        let resp = WebChannelResponse::from_json(json)?;
        FirefoxAccount::from_credentials(*config, client_id, redirect_uri, resp)
    })
}

/// Creates a [FirefoxAccount].
///
/// Note: This takes ownership of `Config`.
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_new(
    config: *mut Config,
    client_id: *const c_char,
    redirect_uri: *const c_char,
    err: *mut ExternError,
) -> *mut FirefoxAccount {
    call_with_result(err, || {
        assert!(!config.is_null());
        let client_id = c_char_to_string(client_id);
        let redirect_uri = c_char_to_string(redirect_uri);
        let config = Box::from_raw(config);
        Ok(FirefoxAccount::new(*config, client_id, redirect_uri))
    })
}

/// Restore a [FirefoxAccount] instance from an serialized state (created with [fxa_to_json]).
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_from_json(
    json: *const c_char,
    err: *mut ExternError,
) -> *mut FirefoxAccount {
    call_with_result(err, || FirefoxAccount::from_json(c_char_to_string(json)))
}

/// Serializes the state of a [FirefoxAccount] instance. It can be restored later with [fxa_from_json].
///
/// It is the responsability of the caller to persist that serialized state regularly (after operations that mutate [FirefoxAccount])
/// in a **secure** location.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_to_json(
    fxa: *mut FirefoxAccount,
    error: *mut ExternError,
) -> *mut c_char {
    call_with_string_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        fxa.to_json()
    })
}

/// Fetches the profile associated with a Firefox Account.
///
/// The profile might get cached in-memory and the caller might get served a cached version.
/// To bypass this, the `ignore_cache` parameter can be set to `true`.
///
/// # Safety
///
/// A destructor [fxa_profile_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_profile(
    fxa: *mut FirefoxAccount,
    ignore_cache: bool,
    error: *mut ExternError,
) -> *mut ProfileC {
    call_with_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        Ok(fxa.get_profile(ignore_cache)?.into())
    })
}

/// Get the Sync token server endpoint URL.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_token_server_endpoint_url(
    fxa: *mut FirefoxAccount,
    error: *mut ExternError,
) -> *mut c_char {
    call_with_string_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        fxa.get_token_server_endpoint_url().map(|u| u.to_string())
    })
}

/// Generate an assertion for a specified audience. Requires to be in a `Married` state.
/// Note that new clients don't use assertions and use Oauth flows instead.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_assertion_new(
    fxa: *mut FirefoxAccount,
    audience: *const c_char,
    error: *mut ExternError,
) -> *mut c_char {
    call_with_string_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        let audience = c_char_to_string(audience);
        fxa.generate_assertion(audience)
    })
}

/// Gets the Sync Keys. Requires to be in a `Married` state.
/// Note that new clients get sync gets using an OAuth flow.
///
/// # Safety
///
/// A destructor [fxa_sync_keys_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_sync_keys(
    fxa: *mut FirefoxAccount,
    error: *mut ExternError,
) -> *mut SyncKeysC {
    call_with_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        let keys: SyncKeysC = fxa.get_sync_keys()?.into();
        Ok(keys)
    })
}

/// Request a OAuth token by starting a new OAuth flow.
///
/// This function returns a URL string that the caller should open in a webview.
///
/// Once the user has confirmed the authorization grant, they will get redirected to `redirect_url`:
/// the caller must intercept that redirection, extract the `code` and `state` query parameters and call
/// [fxa_complete_oauth_flow] to complete the flow.
///
/// It is possible also to request keys (e.g. sync keys) during that flow by setting `wants_keys` to true.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_begin_oauth_flow(
    fxa: *mut FirefoxAccount,
    scope: *const c_char,
    wants_keys: bool,
    error: *mut ExternError,
) -> *mut c_char {
    call_with_string_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        let scope = c_char_to_string(scope);
        let scopes: Vec<&str> = scope.split(" ").collect();
        fxa.begin_oauth_flow(&scopes, wants_keys)
    })
}

/// Finish an OAuth flow initiated by [fxa_begin_oauth_flow] and returns token/keys.
///
/// This resulting token might not have all the `scopes` the caller have requested (e.g. the user
/// might have denied some of them): it is the responsibility of the caller to accomodate that.
///
/// # Safety
///
/// A destructor [fxa_oauth_info_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_complete_oauth_flow(
    fxa: *mut FirefoxAccount,
    code: *const c_char,
    state: *const c_char,
    error: *mut ExternError,
) -> *mut OAuthInfoC {
    call_with_result(error, || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        let code = c_char_to_string(code);
        let state = c_char_to_string(state);
        let info = fxa.complete_oauth_flow(code, state)?;
        Ok(info.into())
    })
}

/// Try to get a previously obtained cached token.
///
/// If the token is expired, the system will try to refresh it automatically using
/// a `refresh_token` or `session_token`.
///
/// If the system can't find a suitable token but has a `session_token`, it will generate a new one on the go.
///
/// If not, the caller must start an OAuth flow with [fxa_begin_oauth_flow].
///
/// # Safety
///
/// A destructor [fxa_oauth_info_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_oauth_token(
    fxa: *mut FirefoxAccount,
    scope: *const c_char,
    error: *mut ExternError,
) -> *mut OAuthInfoC {
    call_with_result_by_value(error, ptr::null_mut(), || {
        assert!(!fxa.is_null());
        let fxa = &mut *fxa;
        let scope = c_char_to_string(scope);
        let scopes: Vec<&str> = scope.split(" ").collect();
        Ok(match fxa.get_oauth_token(&scopes)? {
            Some(info) => Box::into_raw(Box::new(info.into())),
            None => ptr::null_mut(),
        })
    })
}

/// Free a Rust-created string.
#[no_mangle]
pub extern "C" fn fxa_str_free(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        drop(CString::from_raw(s))
    };
}

/// Creates a function with a given `$name` that releases the memory for a type `$t`.
macro_rules! define_destructor (
     ($name:ident, $t:ty) => (
         #[no_mangle]
         pub unsafe extern "C" fn $name(obj: *mut $t) {
             if !obj.is_null() { drop(Box::from_raw(obj)); }
         }
     )
);

define_destructor!(fxa_free, FirefoxAccount);
define_destructor!(fxa_config_free, Config);
define_destructor!(fxa_oauth_info_free, OAuthInfoC);
define_destructor!(fxa_profile_free, ProfileC);
define_destructor!(fxa_sync_keys_free, SyncKeysC);
