/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::ConcurrentHandleMap;
use ffi_support::{
    define_handle_map_deleter, define_string_destructor, rust_str_from_c, rust_string_from_c,
    ExternError,
};
use logins::{Login, PasswordEngine, Result};
use std::os::raw::c_char;
use sync15::telemetry;

fn logging_init() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Filter::default().with_min_level(log::Level::Debug),
            Some("liblogins_ffi"),
        );
        log::debug!("Android logging should be hooked up!")
    }
}

lazy_static::lazy_static! {
    static ref ENGINES: ConcurrentHandleMap<PasswordEngine> = ConcurrentHandleMap::new();
}

#[no_mangle]
pub extern "C" fn sync15_passwords_enable_logcat_logging() {
    #[cfg(target_os = "android")]
    {
        let _ = std::panic::catch_unwind(|| {
            android_logger::init_once(
                android_logger::Filter::default().with_min_level(log::Level::Debug),
                Some("liblogins_ffi"),
            );
            log::debug!("Android logging should be hooked up!")
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new(
    db_path: *const c_char,
    encryption_key: *const c_char,
    error: &mut ExternError,
) -> u64 {
    log::debug!("sync15_passwords_state_new");
    ENGINES.insert_with_result(error, || {
        let path = rust_str_from_c(db_path);
        let key = rust_str_from_c(encryption_key);
        PasswordEngine::new(path, Some(key))
    })
}

unsafe fn bytes_to_key_string(key_bytes: *const u8, len: usize) -> Option<String> {
    if len == 0 {
        log::info!("Opening/Creating unencrypted database!");
        return None;
    } else {
        assert!(
            !key_bytes.is_null(),
            "Null pointer provided with nonzero length"
        );
    }
    let byte_slice = std::slice::from_raw_parts(key_bytes, len);
    Some(base16::encode_lower(byte_slice))
}

/// Same as sync15_passwords_state_new, but automatically hex-encodes the string.
///
/// If a key_len of 0 is provided, then the database will not be encrypted.
///
/// Note: lowercase hex characters are used (e.g. it encodes using the character set 0-9a-f and NOT 0-9A-F).
#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new_with_hex_key(
    db_path: *const c_char,
    encryption_key: *const u8,
    encryption_key_len: u32,
    error: &mut ExternError,
) -> u64 {
    logging_init();
    log::debug!("sync15_passwords_state_new_with_hex_key");
    ENGINES.insert_with_result(error, || {
        let path = rust_str_from_c(db_path);
        let key = bytes_to_key_string(encryption_key, encryption_key_len as usize);
        // We have a Option<String>, but need an Option<&str>...
        let opt_key_ref = key.as_ref().map(|s| s.as_str());
        PasswordEngine::new(path, opt_key_ref)
    })
}

// indirection to help `?` figure out the target error type
fn parse_url(url: &str) -> sync15::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_sync(
    handle: u64,
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_url: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_sync");
    ENGINES.call_with_result(error, handle, |state| -> Result<()> {
        let mut sync_ping = telemetry::SyncTelemetryPing::new();
        let result = state.sync(
            &sync15::Sync15StorageClientInit {
                key_id: rust_string_from_c(key_id),
                access_token: rust_string_from_c(access_token),
                tokenserver_url: parse_url(rust_str_from_c(tokenserver_url))?,
            },
            &sync15::KeyBundle::from_ksync_base64(rust_str_from_c(sync_key))?,
            &mut sync_ping,
        );
        result
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_touch(
    handle: u64,
    id: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_touch");
    ENGINES.call_with_result(error, handle, |state| state.touch(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_delete(
    handle: u64,
    id: *const c_char,
    error: &mut ExternError,
) -> u8 {
    log::debug!("sync15_passwords_delete");
    ENGINES.call_with_result(error, handle, |state| state.delete(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_wipe");
    ENGINES.call_with_result(error, handle, |state| state.wipe())
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe_local(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_wipe_local");
    ENGINES.call_with_result(error, handle, |state| state.wipe_local())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_reset(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_reset");
    ENGINES.call_with_result(error, handle, |state| state.reset())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_get_all(handle: u64, error: &mut ExternError) -> *mut c_char {
    log::debug!("sync15_passwords_get_all");
    ENGINES.call_with_result(error, handle, |state| -> Result<String> {
        let all_passwords = state.list()?;
        let result = serde_json::to_string(&all_passwords)?;
        Ok(result)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_by_id(
    handle: u64,
    id: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("sync15_passwords_get_by_id");
    ENGINES.call_with_result(error, handle, |state| state.get(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_add(
    handle: u64,
    record_json: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("sync15_passwords_add");
    ENGINES.call_with_result(error, handle, |state| {
        let mut parsed: serde_json::Value = serde_json::from_str(rust_str_from_c(record_json))?;
        if parsed.get("id").is_none() {
            // Note: we replace this with a real guid in `db.rs`.
            parsed["id"] = serde_json::Value::String(String::default());
        }
        let login: Login = serde_json::from_value(parsed)?;
        state.add(login)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_update(
    handle: u64,
    record_json: *const c_char,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_update");
    ENGINES.call_with_result(error, handle, |state| {
        let parsed: Login = serde_json::from_str(rust_str_from_c(record_json))?;
        state.update(parsed)
    });
}

define_string_destructor!(sync15_passwords_destroy_string);
define_handle_map_deleter!(ENGINES, sync15_passwords_state_destroy);
