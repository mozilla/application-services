/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
// Let's allow these in the FFI code, since it's usually just a coincidence if
// the closure is small.
#![allow(clippy::redundant_closure)]

use ffi_support::ConcurrentHandleMap;
use ffi_support::{
    define_box_destructor, define_handle_map_deleter, define_string_destructor, ExternError, FfiStr,
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
pub extern "C" fn sync15_passwords_state_new(
    db_path: FfiStr<'_>,
    encryption_key: FfiStr<'_>,
    error: &mut ExternError,
) -> u64 {
    log::debug!("sync15_passwords_state_new");
    ENGINES.insert_with_result(error, || {
        let path = db_path.as_str();
        let key = encryption_key.as_str();
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
    db_path: FfiStr<'_>,
    encryption_key: *const u8,
    encryption_key_len: u32,
    error: &mut ExternError,
) -> u64 {
    logging_init();
    log::debug!("sync15_passwords_state_new_with_hex_key");
    ENGINES.insert_with_result(error, || {
        let path = db_path.as_str();
        let key = bytes_to_key_string(encryption_key, encryption_key_len as usize);
        // We have a Option<String>, but need an Option<&str>...
        let opt_key_ref = key.as_ref().map(String::as_str);
        PasswordEngine::new(path, opt_key_ref)
    })
}

// indirection to help `?` figure out the target error type
fn parse_url(url: &str) -> sync15::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub extern "C" fn sync15_passwords_disable_mem_security(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_disable_mem_security");
    ENGINES.call_with_result(error, handle, |state| -> Result<()> {
        state.disable_mem_security()
    })
}

#[no_mangle]
pub extern "C" fn sync15_passwords_sync(
    handle: u64,
    key_id: FfiStr<'_>,
    access_token: FfiStr<'_>,
    sync_key: FfiStr<'_>,
    tokenserver_url: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_sync");
    ENGINES.call_with_result(error, handle, |state| -> Result<()> {
        let mut sync_ping = telemetry::SyncTelemetryPing::new();
        state.sync(
            &sync15::Sync15StorageClientInit {
                key_id: key_id.into_string(),
                access_token: access_token.into_string(),
                tokenserver_url: parse_url(tokenserver_url.as_str())?,
            },
            &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?,
            &mut sync_ping,
        )
    })
}

#[no_mangle]
pub extern "C" fn sync15_passwords_touch(handle: u64, id: FfiStr<'_>, error: &mut ExternError) {
    log::debug!("sync15_passwords_touch");
    ENGINES.call_with_result(error, handle, |state| state.touch(id.as_str()))
}

#[no_mangle]
pub extern "C" fn sync15_passwords_delete(
    handle: u64,
    id: FfiStr<'_>,
    error: &mut ExternError,
) -> u8 {
    log::debug!("sync15_passwords_delete");
    ENGINES.call_with_result(error, handle, |state| state.delete(id.as_str()))
}

#[no_mangle]
pub extern "C" fn sync15_passwords_wipe(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_wipe");
    ENGINES.call_with_result(error, handle, |state| state.wipe())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_wipe_local(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_wipe_local");
    ENGINES.call_with_result(error, handle, |state| state.wipe_local())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_reset(handle: u64, error: &mut ExternError) {
    log::debug!("sync15_passwords_reset");
    ENGINES.call_with_result(error, handle, |state| state.reset())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_new_interrupt_handle(
    handle: u64,
    error: &mut ExternError,
) -> *mut sql_support::SqlInterruptHandle {
    log::debug!("sync15_passwords_new_interrupt_handle");
    ENGINES.call_with_output(error, handle, |state| state.new_interrupt_handle())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_interrupt(
    handle: &sql_support::SqlInterruptHandle,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_interrupt");
    ffi_support::call_with_output(error, || handle.interrupt())
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
pub extern "C" fn sync15_passwords_get_by_id(
    handle: u64,
    id: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("sync15_passwords_get_by_id");
    ENGINES.call_with_result(error, handle, |state| state.get(id.as_str()))
}

#[no_mangle]
pub extern "C" fn sync15_passwords_add(
    handle: u64,
    record_json: FfiStr<'_>,
    error: &mut ExternError,
) -> *mut c_char {
    log::debug!("sync15_passwords_add");
    ENGINES.call_with_result(error, handle, |state| {
        let mut parsed: serde_json::Value = serde_json::from_str(record_json.as_str())?;
        if parsed.get("id").is_none() {
            // Note: we replace this with a real guid in `db.rs`.
            parsed["id"] = serde_json::Value::String(String::default());
        }
        let login: Login = serde_json::from_value(parsed)?;
        state.add(login)
    })
}

#[no_mangle]
pub extern "C" fn sync15_passwords_update(
    handle: u64,
    record_json: FfiStr<'_>,
    error: &mut ExternError,
) {
    log::debug!("sync15_passwords_update");
    ENGINES.call_with_result(error, handle, |state| {
        let parsed: Login = serde_json::from_str(record_json.as_str())?;
        state.update(parsed)
    });
}

define_string_destructor!(sync15_passwords_destroy_string);
define_handle_map_deleter!(ENGINES, sync15_passwords_state_destroy);
define_box_destructor!(
    sql_support::SqlInterruptHandle,
    sync15_passwords_interrupt_handle_destroy
);
