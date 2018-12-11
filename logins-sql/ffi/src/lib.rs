/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ffi_support::{
    call_with_result, define_box_destructor, define_string_destructor, rust_str_from_c,
    rust_string_from_c, ExternError,
};
use log::*;
use logins_sql::{Login, PasswordEngine, Result};
use std::os::raw::c_char;

fn logging_init() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Filter::default().with_min_level(log::Level::Trace),
            Some("liblogins_ffi"),
        );
        debug!("Android logging should be hooked up!")
    }
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new(
    db_path: *const c_char,
    encryption_key: *const c_char,
    error: &mut ExternError,
) -> *mut PasswordEngine {
    logging_init();
    trace!("sync15_passwords_state_new");
    call_with_result(error, || {
        let path = rust_str_from_c(db_path);
        let key = rust_str_from_c(encryption_key);
        PasswordEngine::new(path, Some(key))
    })
}

// indirection to help `?` figure out the target error type
fn parse_url(url: &str) -> sync15_adapter::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_sync(
    state: &mut PasswordEngine,
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_url: *const c_char,
    error: &mut ExternError,
) {
    trace!("sync15_passwords_sync");
    call_with_result(error, || -> Result<()> {
        state.sync(
            &sync15_adapter::Sync15StorageClientInit {
                key_id: rust_string_from_c(key_id),
                access_token: rust_string_from_c(access_token),
                tokenserver_url: parse_url(rust_str_from_c(tokenserver_url))?,
            },
            &sync15_adapter::KeyBundle::from_ksync_base64(rust_str_from_c(sync_key))?,
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_touch(
    state: &PasswordEngine,
    id: *const c_char,
    error: &mut ExternError,
) {
    trace!("sync15_passwords_touch");
    call_with_result(error, || state.touch(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_delete(
    state: &PasswordEngine,
    id: *const c_char,
    error: &mut ExternError,
) -> u8 {
    trace!("sync15_passwords_delete");
    call_with_result(error, || state.delete(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe(state: &PasswordEngine, error: &mut ExternError) {
    trace!("sync15_passwords_wipe");
    call_with_result(error, || state.wipe())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_reset(state: &PasswordEngine, error: &mut ExternError) {
    trace!("sync15_passwords_reset");
    call_with_result(error, || state.reset())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_get_all(
    state: &PasswordEngine,
    error: &mut ExternError,
) -> *mut c_char {
    trace!("sync15_passwords_get_all");
    call_with_result(error, || -> Result<String> {
        let all_passwords = state.list()?;
        let result = serde_json::to_string(&all_passwords)?;
        Ok(result)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_by_id(
    state: &PasswordEngine,
    id: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    trace!("sync15_passwords_get_by_id");
    call_with_result(error, || state.get(rust_str_from_c(id)))
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_add(
    state: &PasswordEngine,
    record_json: *const c_char,
    error: &mut ExternError,
) -> *mut c_char {
    trace!("sync15_passwords_add");
    call_with_result(error, || {
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
    state: &PasswordEngine,
    record_json: *const c_char,
    error: &mut ExternError,
) {
    trace!("sync15_passwords_update");
    call_with_result(error, || {
        let parsed: Login = serde_json::from_str(rust_str_from_c(record_json))?;
        state.update(parsed)
    });
}

define_string_destructor!(sync15_passwords_destroy_string);
define_box_destructor!(PasswordEngine, sync15_passwords_state_destroy);
