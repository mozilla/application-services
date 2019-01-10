/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::{
    call_with_output, call_with_result, define_box_destructor, define_string_destructor,
    rust_str_from_c, ExternError,
};
use fxa_client::{ffi::*, FirefoxAccount, PersistCallback};
use std::{ffi::CString, os::raw::c_char};

fn logging_init() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Filter::default().with_min_level(log::Level::Trace),
            Some("libfxaclient_ffi"),
        );
        log::debug!("Android logging should be hooked up!")
    }
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
) -> *mut FirefoxAccount {
    use fxa_client::WebChannelResponse;
    logging_init();
    log::trace!("fxa_from_credentials");
    call_with_result(err, || {
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
) -> *mut FirefoxAccount {
    logging_init();
    log::trace!("fxa_new");
    call_with_output(err, || {
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
pub unsafe extern "C" fn fxa_from_json(
    json: *const c_char,
    err: &mut ExternError,
) -> *mut FirefoxAccount {
    logging_init();
    log::trace!("fxa_from_json");
    call_with_result(err, || FirefoxAccount::from_json(rust_str_from_c(json)))
}

/// Serializes the state of a [FirefoxAccount] instance. It can be restored later with [fxa_from_json].
///
/// It is the responsability of the caller to persist that serialized state regularly (after operations that mutate [FirefoxAccount])
/// in a **secure** location or to use [fxa_register_persist_callback].
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_to_json(fxa: &mut FirefoxAccount, error: &mut ExternError) -> *mut c_char {
    log::trace!("fxa_to_json");
    call_with_result(error, || fxa.to_json())
}

/// Registers a callback that gets called every time the FirefoxAccount internal state
/// changed and therefore need to be persisted.
#[no_mangle]
pub unsafe extern "C" fn fxa_register_persist_callback(
    fxa: &mut FirefoxAccount,
    callback: extern "C" fn(json: *const c_char),
    error: &mut ExternError,
) {
    log::trace!("fxa_register_persist_callback");
    call_with_output(error, || {
        fxa.register_persist_callback(PersistCallback::new(move |json| {
            // It's impossible for JSON to have embedded null bytes.
            let s = CString::new(json).unwrap();
            callback(s.as_ptr());
        }));
    });
}

/// Unregisters a previous registered persist callback
#[no_mangle]
pub extern "C" fn fxa_unregister_persist_callback(
    fxa: &mut FirefoxAccount,
    error: &mut ExternError,
) {
    log::trace!("fxa_unregister_persist_callback");
    call_with_output(error, || {
        fxa.unregister_persist_callback();
    });
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
    fxa: &mut FirefoxAccount,
    ignore_cache: bool,
    error: &mut ExternError,
) -> *mut ProfileC {
    log::trace!("fxa_profile");
    call_with_result(error, || fxa.get_profile(ignore_cache))
}

/// Get the Sync token server endpoint URL.
///
/// # Safety
///
/// A destructor [fxa_str_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub extern "C" fn fxa_get_token_server_endpoint_url(
    fxa: &FirefoxAccount,
    error: &mut ExternError,
) -> *mut c_char {
    log::trace!("fxa_get_token_server_endpoint_url");
    call_with_result(error, || {
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
    fxa: &FirefoxAccount,
    error: &mut ExternError,
) -> *mut c_char {
    log::trace!("fxa_get_connection_success_url");
    call_with_result(error, || {
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
    fxa: &mut FirefoxAccount,
    audience: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::trace!("fxa_assertion_new");
    call_with_result(error, || {
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
pub extern "C" fn fxa_get_sync_keys(
    fxa: &mut FirefoxAccount,
    error: &mut ExternError,
) -> *mut SyncKeysC {
    log::trace!("fxa_get_sync_keys");
    call_with_result(error, || fxa.get_sync_keys())
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
    fxa: &mut FirefoxAccount,
    pairing_url: *const c_char,
    scope: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::trace!("fxa_begin_pairing_flow");
    call_with_result(error, || {
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
    fxa: &mut FirefoxAccount,
    scope: *const c_char,
    wants_keys: bool,
    error: &mut ExternError,
) -> *mut c_char {
    log::trace!("fxa_begin_oauth_flow");
    call_with_result(error, || {
        let scope = rust_str_from_c(scope);
        let scopes: Vec<&str> = scope.split(" ").collect();
        fxa.begin_oauth_flow(&scopes, wants_keys)
    })
}

/// Finish an OAuth flow initiated by [fxa_begin_oauth_flow].
///
/// # Safety
///
/// A destructor [fxa_oauth_info_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_complete_oauth_flow(
    fxa: &mut FirefoxAccount,
    code: *const c_char,
    state: *const c_char,
    error: &mut ExternError,
) {
    log::trace!("fxa_complete_oauth_flow");
    call_with_result(error, || {
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
/// A destructor [fxa_oauth_info_free] is provided for releasing the memory for this
/// pointer type.
#[no_mangle]
pub unsafe extern "C" fn fxa_get_access_token(
    fxa: &mut FirefoxAccount,
    scope: *const c_char,
    error: &mut ExternError,
) -> *mut AccessTokenInfoC {
    log::trace!("fxa_get_access_token");
    call_with_result(error, || {
        let scope = rust_str_from_c(scope);
        fxa.get_access_token(&scope)
    })
}

define_string_destructor!(fxa_str_free);

define_box_destructor!(FirefoxAccount, fxa_free);
define_box_destructor!(AccessTokenInfoC, fxa_oauth_info_free);
define_box_destructor!(ProfileC, fxa_profile_free);
define_box_destructor!(SyncKeysC, fxa_sync_keys_free);
