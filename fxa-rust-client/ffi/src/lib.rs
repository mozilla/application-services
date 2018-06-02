/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate fxa_client;
extern crate libc;

mod ctypes;
mod util;

use std::ffi::CString;

use ctypes::*;
use fxa_client::errors::Error as InternalError;
use fxa_client::errors::ErrorKind::*;
use fxa_client::{Config, FirefoxAccount, FxAWebChannelResponse};
use libc::{c_char, c_void};
use util::*;

#[repr(C)]
#[derive(Debug)]
pub enum ErrorCode {
    Other,
    AuthenticationError,
}

#[repr(C)]
#[derive(Debug)]
pub struct ExternError {
    code: ErrorCode,
    message: *const c_char,
}

/// A C representation Rust's [Result](std::result::Result).
/// A value of `Ok` results in `ok` containing a raw pointer as a `c_void`
/// and `err` containing a null pointer.
/// A value of `Err` results in `value` containing a null pointer and `err` containing an error struct.
///
/// #Safety
///
/// Callers are responsible for managing the memory for the return value.
/// A destructor `destroy` is provided for releasing the memory for this
/// pointer type.
#[repr(C)]
#[derive(Debug)]
pub struct ExternResult {
    pub ok: *const c_void, // We could have used `*const T` instead, but that would have meant creating one `free` function per variant.
    pub err: *const ExternError,
}

impl ExternResult {
    pub fn ok<T>(result: T) -> *mut Self {
        Box::into_raw(Box::new(ExternResult {
            ok: Box::into_raw(Box::new(result)) as *const _ as *const c_void,
            err: std::ptr::null_mut(),
        }))
    }

    pub fn ok_null() -> *mut Self {
        Box::into_raw(Box::new(ExternResult {
            ok: std::ptr::null_mut(),
            err: std::ptr::null_mut(),
        }))
    }

    fn err<S>(code: ErrorCode, msg: S) -> *mut Self
    where
        S: Into<String>,
    {
        Box::into_raw(Box::new(ExternResult {
            ok: std::ptr::null_mut(),
            err: Box::into_raw(Box::new(ExternError {
                code,
                message: string_to_c_char(msg),
            })),
        }))
    }

    pub fn from_internal(err: InternalError) -> *mut Self {
        match err {
            InternalError(RemoteError(_, 401, ..), ..) | InternalError(NotMarried, ..) => {
                ExternResult::err(ErrorCode::AuthenticationError, err.to_string())
            }
            _ => ExternResult::err(ErrorCode::Other, err.to_string()),
        }
    }
}

/// Convenience function over [fxa_get_custom_config] that provides a pointer to a [Config] that
/// points to the production FxA servers.
#[no_mangle]
pub extern "C" fn fxa_get_release_config() -> *mut ExternResult {
    match Config::release() {
        Ok(config) => ExternResult::ok(config),
        Err(err) => ExternResult::from_internal(err),
    }
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
pub extern "C" fn fxa_get_custom_config(content_base: *const c_char) -> *mut ExternResult {
    let content_base = c_char_to_string(content_base);
    match Config::import_from(content_base) {
        Ok(config) => ExternResult::ok(config),
        Err(err) => ExternResult::from_internal(err),
    }
}

/// Creates a [FirefoxAccount] from credentials obtained with the "session-token" FxA
/// login flow.
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
pub extern "C" fn fxa_from_credentials(
    config: *mut Config,
    client_id: *const c_char,
    json: *const c_char,
) -> *mut ExternResult {
    let config = unsafe { Box::from_raw(&mut *config) };
    let json = c_char_to_string(json);
    let client_id = c_char_to_string(client_id);
    let resp = match FxAWebChannelResponse::from_json(json) {
        Ok(resp) => resp,
        Err(err) => return ExternResult::from_internal(err),
    };
    let fxa = match FirefoxAccount::from_credentials(*config, client_id, resp) {
        Ok(fxa) => fxa,
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(fxa)
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
pub extern "C" fn fxa_new(config: *mut Config, client_id: *const c_char) -> *mut ExternResult {
    let client_id = c_char_to_string(client_id);
    let config = unsafe { Box::from_raw(&mut *config) };
    let fxa = FirefoxAccount::new(*config, client_id);
    ExternResult::ok(fxa)
}

/// Restore a [FirefoxAccount] instance from an serialized state (created with [fxa_to_json]).
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_from_json(json: *const c_char) -> *mut ExternResult {
    let json = c_char_to_string(json);
    let fxa = match FirefoxAccount::from_json(json) {
        Ok(fxa) => fxa,
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(fxa)
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
pub extern "C" fn fxa_to_json(fxa: *mut FirefoxAccount) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let json = match fxa.to_json() {
        Ok(json) => json,
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(json)
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
pub extern "C" fn fxa_profile(
    fxa: *mut FirefoxAccount,
    profile_access_token: &str,
    ignore_cache: bool,
) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let profile: ProfileC = match fxa.get_profile(profile_access_token, ignore_cache) {
        Ok(profile) => profile.into(),
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(profile)
}

/// Get the Sync token server endpoint URL.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_get_token_server_endpoint_url(fxa: *mut FirefoxAccount) -> *mut c_char {
    let fxa = unsafe { &mut *fxa };
    let url = fxa.get_token_server_endpoint_url();
    string_to_c_char(url)
}

/// Generate an assertion for a specified audience. Requires to be in a `Married` state.
/// Note that new clients don't use assertions and use Oauth flows instead.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_assertion_new(
    fxa: *mut FirefoxAccount,
    audience: *const c_char,
) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let audience = c_char_to_string(audience);
    let assertion = match fxa.generate_assertion(audience) {
        Ok(assertion) => string_to_c_char(assertion),
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(assertion)
}

/// Gets the Sync Keys. Requires to be in a `Married` state.
/// Note that new clients get sync gets using an OAuth flow.
///
/// # Safety
///
/// A destructor [fxa_sync_keys_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_get_sync_keys(fxa: *mut FirefoxAccount) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let sync_keys: SyncKeysC = match fxa.get_sync_keys() {
        Ok(sync_keys) => sync_keys.into(),
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(sync_keys)
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
pub extern "C" fn fxa_begin_oauth_flow(
    fxa: *mut FirefoxAccount,
    redirect_uri: *const c_char,
    scope: *const c_char,
    wants_keys: bool,
) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let redirect_uri = c_char_to_string(redirect_uri);
    let scope = c_char_to_string(scope);
    let scopes: Vec<&str> = scope.split(" ").collect();
    let oauth_flow = match fxa.begin_oauth_flow(redirect_uri, &scopes, wants_keys) {
        Ok(oauth_flow) => string_to_c_char(oauth_flow),
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(oauth_flow)
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
pub extern "C" fn fxa_complete_oauth_flow(
    fxa: *mut FirefoxAccount,
    code: *const c_char,
    state: *const c_char,
) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let code = c_char_to_string(code);
    let state = c_char_to_string(state);
    let info: OAuthInfoC = match fxa.complete_oauth_flow(code, state) {
        Ok(info) => info.into(),
        Err(err) => return ExternResult::from_internal(err),
    };
    ExternResult::ok(info)
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
pub extern "C" fn fxa_get_oauth_token(
    fxa: *mut FirefoxAccount,
    scope: *const c_char,
) -> *mut ExternResult {
    let fxa = unsafe { &mut *fxa };
    let scope = c_char_to_string(scope);
    let scopes: Vec<&str> = scope.split(" ").collect();
    let auth_info = match fxa.get_oauth_token(&scopes) {
        Ok(oauth_info) => oauth_info,
        Err(err) => return ExternResult::from_internal(err),
    };
    match auth_info {
        Some(oauth_info) => {
            let oauth_info: OAuthInfoC = oauth_info.into();
            ExternResult::ok(oauth_info)
        }
        None => ExternResult::ok_null(),
    }
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

define_destructor!(free_extern_result, ExternResult);
define_destructor!(free_extern_error, ExternError);
define_destructor!(fxa_free, FirefoxAccount);
define_destructor!(fxa_config_free, Config);
define_destructor!(fxa_oauth_info_free, OAuthInfoC);
define_destructor!(fxa_profile_free, ProfileC);
define_destructor!(fxa_sync_keys_free, SyncKeysC);
