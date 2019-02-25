/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "browserid")]
use ffi_support::define_box_destructor;
use ffi_support::{
    define_bytebuffer_destructor, define_handle_map_deleter, define_string_destructor,
    rust_str_from_c, ByteBuffer, ConcurrentHandleMap, ExternError,
};
#[cfg(feature = "browserid")]
use fxa_client::ffi::SyncKeysC;
use fxa_client::FirefoxAccount;
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn fxa_enable_logcat_logging() {
    #[cfg(target_os = "android")]
    {
        let _ = std::panic::catch_unwind(|| {
            android_logger::init_once(
                android_logger::Filter::default().with_min_level(log::Level::Debug),
                Some("libfxaclient_ffi"),
            );
            log::debug!("Android logging should be hooked up!")
        });
    }
}

lazy_static::lazy_static! {
    static ref ACCOUNTS: ConcurrentHandleMap<FirefoxAccount> = ConcurrentHandleMap::new();
}

/// Creates a [FirefoxAccount] from credentials obtained with the onepw FxA login flow.
///
/// This is typically used by the legacy Sync clients: new clients mainly use OAuth flows and
/// therefore should use `fxa_new`.
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[cfg(feature = "browserid")]
#[no_mangle]
pub unsafe extern "C" fn fxa_from_credentials(
    content_url: *const c_char,
    client_id: *const c_char,
    redirect_uri: *const c_char,
    json: *const c_char,
    err: &mut ExternError,
) -> u64 {
    use fxa_client::WebChannelResponse;
    log::debug!("fxa_from_credentials");
    ACCOUNTS.insert_with_result(err, || {
        let content_url = rust_str_from_c(content_url);
        let client_id = rust_str_from_c(client_id);
        let redirect_uri = rust_str_from_c(redirect_uri);
        let json = rust_str_from_c(json);
        let resp = WebChannelResponse::from_json(json)?;
        FirefoxAccount::from_credentials(content_url, client_id, redirect_uri, resp)
    })
}

/// Creates a [FirefoxAccount].
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_new(
    content_url: *const c_char,
    client_id: *const c_char,
    redirect_uri: *const c_char,
    err: &mut ExternError,
) -> u64 {
    log::debug!("fxa_new");
    ACCOUNTS.insert_with_output(err, || {
        let content_url = rust_str_from_c(content_url);
        let client_id = rust_str_from_c(client_id);
        let redirect_uri = rust_str_from_c(redirect_uri);
        FirefoxAccount::new(content_url, client_id, redirect_uri)
    })
}

/// Restore a [FirefoxAccount] instance from an serialized state (created with [fxa_to_json]).
///
/// # Safety
///
/// A destructor [fxa_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_from_json(json: *const c_char, err: &mut ExternError) -> u64 {
    log::debug!("fxa_from_json");
    ACCOUNTS.insert_with_result(err, || FirefoxAccount::from_json(rust_str_from_c(json)))
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
pub extern "C" fn fxa_to_json(handle: u64, error: &mut ExternError) -> *mut c_char {
    log::debug!("fxa_to_json");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| fxa.to_json())
}

/// Fetches the profile associated with a Firefox Account.
///
/// The profile might get cached in-memory and the caller might get served a cached version.
/// To bypass this, the `ignore_cache` parameter can be set to `true`.
///
/// # Safety
///
/// A destructor [fxa_bytebuffer_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_profile(
    handle: u64,
    ignore_cache: bool,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("fxa_profile");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| fxa.get_profile(ignore_cache))
}

/// Get the Sync token server endpoint URL.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_get_token_server_endpoint_url(
    handle: u64,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_get_token_server_endpoint_url");
    ACCOUNTS.call_with_result(error, handle, |fxa| {
        fxa.get_token_server_endpoint_url().map(|u| u.to_string())
    })
}

/// Get the url to redirect after there has been a successful connection to FxA.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_get_connection_success_url(
    handle: u64,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_get_connection_success_url");
    ACCOUNTS.call_with_result(error, handle, |fxa| {
        fxa.get_connection_success_url().map(|u| u.to_string())
    })
}

/// Generate an assertion for a specified audience. Requires to be in a `Married` state.
/// Note that new clients don't use assertions and use Oauth flows instead.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[cfg(feature = "browserid")]
#[no_mangle]
pub unsafe extern "C" fn fxa_assertion_new(
    handle: u64,
    audience: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_assertion_new");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| {
        let audience = rust_str_from_c(audience);
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
#[cfg(feature = "browserid")]
#[no_mangle]
pub extern "C" fn fxa_get_sync_keys(handle: u64, error: &mut ExternError) -> *mut SyncKeysC {
    log::debug!("fxa_get_sync_keys");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| fxa.get_sync_keys())
}

/// Request a OAuth token by starting a new pairing flow, by calling the content server pairing endpoint.
///
/// This function returns a URL string that the caller should open in a webview.
///
/// Pairing assumes you want keys by default, so you must provide a key-bearing scope.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_begin_pairing_flow(
    handle: u64,
    pairing_url: *const c_char,
    scope: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_begin_pairing_flow");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| {
        let pairing_url = rust_str_from_c(pairing_url);
        let scope = rust_str_from_c(scope);
        let scopes: Vec<&str> = scope.split(" ").collect();
        fxa.begin_pairing_flow(&pairing_url, &scopes)
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
    handle: u64,
    scope: *const c_char,
    wants_keys: bool,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("fxa_begin_oauth_flow");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| {
        let scope = rust_str_from_c(scope);
        let scopes: Vec<&str> = scope.split(" ").collect();
        fxa.begin_oauth_flow(&scopes, wants_keys)
    })
}

/// Finish an OAuth flow initiated by [fxa_begin_oauth_flow].
#[no_mangle]
pub unsafe extern "C" fn fxa_complete_oauth_flow(
    handle: u64,
    code: *const c_char,
    state: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("fxa_complete_oauth_flow");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| {
        let code = rust_str_from_c(code);
        let state = rust_str_from_c(state);
        fxa.complete_oauth_flow(code, state)
    });
}

/// Try to get an access token.
///
/// If the system can't find a suitable token but has a `refresh token` or a `session_token`,
/// it will generate a new one on the go.
///
/// If not, the caller must start an OAuth flow with [fxa_begin_oauth_flow].
///
/// # Safety
///
/// A destructor [fxa_bytebuffer_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_access_token(
    handle: u64,
    scope: *const c_char,
    error: &mut ExternError,
) -> ByteBuffer {
    log::debug!("fxa_get_access_token");
    ACCOUNTS.call_with_result_mut(error, handle, |fxa| {
        let scope = rust_str_from_c(scope);
        fxa.get_access_token(&scope)
    })
}

define_string_destructor!(fxa_str_free);

define_handle_map_deleter!(ACCOUNTS, fxa_free);
#[cfg(feature = "browserid")]
define_box_destructor!(SyncKeysC, fxa_sync_keys_free);
define_bytebuffer_destructor!(fxa_bytebuffer_free);
