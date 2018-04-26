extern crate fxa_client;
extern crate libc;

mod ctypes;
mod util;

use std::ffi::CString;

use fxa_client::{FirefoxAccount, FxAWebChannelResponse, FxAConfig};
use ctypes::*;
use util::*;
use libc::c_char;

#[no_mangle]
pub extern "C" fn fxa_config_release() -> *mut FxAConfig {
  let config = FxAConfig::release();
  Box::into_raw(Box::new(config))
}

/// Note: After calling this function, Rust will now own `config`, therefore the caller's
/// pointer should be dropped.
#[no_mangle]
pub extern "C" fn fxa_from_credentials(config: *mut FxAConfig, json: *const c_char)
  -> *mut FirefoxAccount {
  let config = unsafe {
      assert!(!config.is_null());
      &mut *config
  };
  let config = unsafe { Box::from_raw(config) };
  let json = c_char_to_string(json);
  let resp = match FxAWebChannelResponse::from_json(&json) {
    Ok(resp) => resp,
    Err(_) => return std::ptr::null_mut()
  };
  let fxa = match FirefoxAccount::from_credentials(*config, resp) {
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
  let sync_keys = match fxa.get_sync_keys() {
    Ok(sync_keys) => sync_keys,
    Err(_) => return std::ptr::null_mut()
  };
  Box::into_raw(Box::new(sync_keys.into()))
}