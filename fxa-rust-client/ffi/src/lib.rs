extern crate fxa_client;
extern crate libc;

mod ctypes;
mod util;

use std::ffi::CString;

use ctypes::*;
use fxa_client::{Config, FirefoxAccount, FxAWebChannelResponse};
use libc::c_char;
use util::*;

#[no_mangle]
pub extern "C" fn fxa_get_release_config() -> *mut Config {
    let config = match Config::release() {
        Ok(config) => config,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(config))
}

#[no_mangle]
pub extern "C" fn fxa_get_custom_config(content_base: *const c_char) -> *mut Config {
    let content_base = c_char_to_string(content_base);
    let config = match Config::import_from(content_base) {
        Ok(config) => config,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(config))
}

/// Note: After calling this function, Rust will now own `config`, therefore the caller's
/// pointer should be dropped.
#[no_mangle]
pub extern "C" fn fxa_from_credentials(
    config: *mut Config,
    client_id: *const c_char,
    json: *const c_char,
) -> *mut FirefoxAccount {
    let config = unsafe {
        assert!(!config.is_null());
        &mut *config
    };
    let config = unsafe { Box::from_raw(config) };
    let json = c_char_to_string(json);
    let client_id = c_char_to_string(client_id);
    let resp = match FxAWebChannelResponse::from_json(json) {
        Ok(resp) => resp,
        Err(_) => return std::ptr::null_mut(),
    };
    let fxa = match FirefoxAccount::from_credentials(*config, client_id, resp) {
        Ok(fxa) => fxa,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(fxa))
}

/// Note: After calling this function, Rust will now own `config`, therefore the caller's
/// pointer should be dropped.
#[no_mangle]
pub extern "C" fn fxa_new(config: *mut Config, client_id: *const c_char) -> *mut FirefoxAccount {
    let config = unsafe {
        assert!(!config.is_null());
        &mut *config
    };
    let client_id = c_char_to_string(client_id);
    let config = unsafe { Box::from_raw(config) };
    Box::into_raw(Box::new(FirefoxAccount::new(*config, client_id)))
}

#[no_mangle]
pub extern "C" fn fxa_from_json(json: *const c_char) -> *mut FirefoxAccount {
    let json = c_char_to_string(json);
    let fxa = match FirefoxAccount::from_json(json) {
        Ok(fxa) => fxa,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(fxa))
}

/// The caller should de-allocate the result using fxa_free_str after use.
#[no_mangle]
pub extern "C" fn fxa_to_json(fxa: *mut FirefoxAccount) -> *mut c_char {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let json = match fxa.to_json() {
        Ok(json) => json,
        Err(_) => return std::ptr::null_mut(),
    };
    string_to_c_char(json)
}

#[no_mangle]
pub extern "C" fn fxa_profile(fxa: *mut FirefoxAccount) -> *mut ProfileC {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let profile = match fxa.get_profile() {
        Ok(profile) => profile,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(profile.into()))
}

/// The caller should de-allocate the result using fxa_free_str after use.
#[no_mangle]
pub extern "C" fn fxa_assertion_new(
    fxa: *mut FirefoxAccount,
    audience: *const c_char,
) -> *mut c_char {
    let audience = c_char_to_string(audience);
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let assertion = match fxa.generate_assertion(audience) {
        Ok(assertion) => assertion,
        Err(_) => return std::ptr::null_mut(),
    };
    string_to_c_char(assertion)
}

#[no_mangle]
pub extern "C" fn fxa_get_sync_keys(fxa: *mut FirefoxAccount) -> *mut SyncKeysC {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let sync_keys = match fxa.get_sync_keys() {
        Ok(sync_keys) => sync_keys,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(sync_keys.into()))
}

/// The caller should de-allocate the result using fxa_free_str after use.
#[no_mangle]
pub extern "C" fn fxa_begin_oauth_flow(
    fxa: *mut FirefoxAccount,
    redirect_uri: *const c_char,
    scope: *const c_char,
    wants_keys: bool,
) -> *mut c_char {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let redirect_uri = c_char_to_string(redirect_uri);
    let scope = c_char_to_string(scope);
    let scopes: Vec<&str> = scope.split(" ").collect();
    let oauth_flow = match fxa.begin_oauth_flow(redirect_uri, &scopes, wants_keys) {
        Ok(oauth_flow) => oauth_flow,
        Err(_) => return std::ptr::null_mut(),
    };
    string_to_c_char(oauth_flow)
}

#[no_mangle]
pub extern "C" fn fxa_complete_oauth_flow(
    fxa: *mut FirefoxAccount,
    code: *const c_char,
    state: *const c_char,
) -> *mut OAuthInfoC {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let code = c_char_to_string(code);
    let state = c_char_to_string(state);
    let info = match fxa.complete_oauth_flow(code, state) {
        Ok(info) => info,
        Err(_) => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(info.into()))
}

#[no_mangle]
pub extern "C" fn fxa_get_oauth_token(
    fxa: *mut FirefoxAccount,
    scope: *const c_char,
) -> *mut OAuthInfoC {
    let fxa = unsafe {
        assert!(!fxa.is_null());
        &mut *fxa
    };
    let scope = c_char_to_string(scope);
    let scopes: Vec<&str> = scope.split(" ").collect();
    let auth_info = match fxa.get_oauth_token(&scopes) {
        Ok(oauth_info) => oauth_info,
        Err(_) => return std::ptr::null_mut(),
    };
    match auth_info {
        Some(oauth_info) => Box::into_raw(Box::new(oauth_info.into())),
        None => return std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn fxa_free_str(s: *mut c_char) {
    unsafe {
        if s.is_null() {
            return;
        }
        CString::from_raw(s)
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
