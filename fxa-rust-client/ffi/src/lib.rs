extern crate fxa_client;
extern crate libc;

use std::ffi::{CString, CStr};
use http_client::{FirefoxAccount, FxAWebChannelResponse, FxAConfig};
use libc::c_char;

#[no_mangle]
pub extern "C" fn fxa_config_release() -> *mut FxAConfigC {
  let config = FxAConfigC {
    auth_url: string_to_c_char("https://api.accounts.firefox.com/v1"),
    oauth_url: string_to_c_char("https://oauth.accounts.firefox.com/v1"),
    profile_url: string_to_c_char("https://oauth.accounts.firefox.com/v1")
  };
  Box::into_raw(Box::new(config))
}

#[no_mangle]
pub extern "C" fn fxa_from_credentials(config: *mut FxAConfigC, json: *const c_char)
  -> *mut FirefoxAccount {
  let config = unsafe {
      assert!(!config.is_null());
      &mut *config
  };
  let config = FxAConfig {
    auth_url: c_char_to_string(config.auth_url),
    oauth_url: c_char_to_string(config.oauth_url),
    profile_url: c_char_to_string(config.profile_url)
  };
  let json = c_char_to_string(json);
  let resp = match FxAWebChannelResponse::from_json(&json) {
    Ok(resp) => resp,
    Err(_) => return std::ptr::null_mut()
  };
  let fxa = match FirefoxAccount::from_credentials(config, resp) {
    Ok(fxa) => fxa,
    Err(_) => return std::ptr::null_mut()
  };
  Box::into_raw(Box::new(fxa))
}

#[no_mangle]
pub unsafe extern "C" fn fxa_free(fxa: *mut FirefoxAccount) {
  let _ = Box::from_raw(fxa);
}

#[no_mangle]
pub extern "C" fn fxa_assertion_new(fxa: *mut FirefoxAccount, audience: *const c_char)
  -> *mut c_char {
  let audience = c_char_to_string(audience);
  let fxa = unsafe {
      assert!(!fxa.is_null());
      &mut *fxa
  };
  let assertion = match fxa.generate_assertion(&audience) {
    Ok(assertion) => assertion,
    Err(_) => return std::ptr::null_mut()
  };
  string_to_c_char(assertion)
}

#[no_mangle]
pub extern "C" fn fxa_assertion_free(s: *mut c_char) {
  unsafe {
    if s.is_null() { return }
    CString::from_raw(s)
  };
}

#[no_mangle]
pub extern "C" fn fxa_get_sync_keys(fxa: *mut FirefoxAccount) -> *mut SyncKeysC {
  let fxa = unsafe {
      assert!(!fxa.is_null());
      &mut *fxa
  };
  let (sync_key, xcs) = match fxa.get_sync_keys() {
    Ok((sync_key, xcs)) => (sync_key, xcs),
    Err(_) => return std::ptr::null_mut()
  };
  let sync_keys = SyncKeysC {
    sync_key: string_to_c_char(sync_key),
    xcs: string_to_c_char(xcs)
  };
  Box::into_raw(Box::new(sync_keys))
}

pub fn c_char_to_string(cchar: *const c_char) -> String {
  let c_str = unsafe { CStr::from_ptr(cchar) };
  let r_str = c_str.to_str().unwrap_or("");
  r_str.to_string()
}

pub fn string_to_c_char<T>(r_string: T) -> *mut c_char where T: Into<String> {
  CString::new(r_string.into()).unwrap().into_raw()
}

#[repr(C)]
pub struct SyncKeysC {
  pub sync_key: *mut c_char,
  pub xcs: *mut c_char
}

#[repr(C)]
pub struct FxAConfigC {
  pub auth_url: *mut c_char,
  pub oauth_url: *mut c_char,
  pub profile_url: *mut c_char,
}
