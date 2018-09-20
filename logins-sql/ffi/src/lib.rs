/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde_json;
extern crate rusqlite;
extern crate logins_sql;
extern crate sync15_adapter;
extern crate url;
#[macro_use] extern crate log;

#[cfg(target_os = "android")]
extern crate android_logger;

pub mod error;

use std::os::raw::c_char;
use std::ffi::{CString, CStr};

use error::{
    ExternError,
    with_translated_result,
    with_translated_value_result,
    with_translated_void_result,
    with_translated_string_result,
    with_translated_opt_string_result,
};

use logins_sql::{
    Login,
    PasswordEngine,
};

#[inline]
unsafe fn c_str_to_str<'a>(cstr: *const c_char) -> &'a str {
    CStr::from_ptr(cstr).to_str().unwrap_or_default()
}

fn logging_init() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Filter::default().with_min_level(log::Level::Trace),
            Some("libloginsapi_ffi"));
        debug!("Android logging should be hooked up!")
    }
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_new(
    db_path: *const c_char,
    encryption_key: *const c_char,
    error: *mut ExternError
) -> *mut PasswordEngine {
    logging_init();
    trace!("sync15_passwords_state_new");
    with_translated_result(error, || {
        let path = c_str_to_str(db_path);
        let key = c_str_to_str(encryption_key);
        let state = PasswordEngine::new(path, Some(key))?;
        Ok(state)
    })
}

// indirection to help `?` figure out the target error type
fn parse_url(url: &str) -> sync15_adapter::Result<url::Url> {
    Ok(url::Url::parse(url)?)
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_sync(
    state: *mut PasswordEngine,
    key_id: *const c_char,
    access_token: *const c_char,
    sync_key: *const c_char,
    tokenserver_url: *const c_char,
    error: *mut ExternError
) {
    trace!("sync15_passwords_sync");
    with_translated_void_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_sync");
        let state = &mut *state;
        state.sync(
            &sync15_adapter::Sync15StorageClientInit {
                key_id: c_str_to_str(key_id).into(),
                access_token: c_str_to_str(access_token).into(),
                tokenserver_url: parse_url(c_str_to_str(tokenserver_url))?,
            },
            &sync15_adapter::KeyBundle::from_ksync_base64(
                c_str_to_str(sync_key).into()
            )?
        )
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_touch(
    state: *const PasswordEngine,
    id: *const c_char,
    error: *mut ExternError
) {
    trace!("sync15_passwords_touch");
    with_translated_void_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_touch");
        let state = &*state;
        state.touch(c_str_to_str(id))
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_delete(
    state: *const PasswordEngine,
    id: *const c_char,
    error: *mut ExternError
) -> u8 {
    trace!("sync15_passwords_delete");
    with_translated_value_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_delete");
        let state = &*state;
        let deleted = state.delete(c_str_to_str(id))?;
        Ok(if deleted { 1 } else { 0 })
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_wipe(
    state: *const PasswordEngine,
    error: *mut ExternError
) {
    trace!("sync15_passwords_wipe");
    with_translated_void_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_wipe");
        let state = &*state;
        state.wipe()
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_reset(
    state: *const PasswordEngine,
    error: *mut ExternError
) {
    trace!("sync15_passwords_reset");
    with_translated_void_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_reset");
        let state = &*state;
        state.reset()
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_all(
    state: *const PasswordEngine,
    error: *mut ExternError
) -> *mut c_char {
    trace!("sync15_passwords_get_all");
    with_translated_string_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_get_all");
        let state = &*state;
        let all_passwords = state.list()?;
        let result = serde_json::to_string(&all_passwords)?;
        Ok(result)
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_get_by_id(
    state: *const PasswordEngine,
    id: *const c_char,
    error: *mut ExternError
) -> *mut c_char {
    trace!("sync15_passwords_get_by_id");
    with_translated_opt_string_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_get_by_id");
        let state = &*state;
        if let Some(password) = state.get(c_str_to_str(id))? {
            Ok(Some(serde_json::to_string(&password)?))
        } else {
            Ok(None)
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_add(
    state: *const PasswordEngine,
    record_json: *const c_char,
    error: *mut ExternError
) -> *mut c_char {
    trace!("sync15_passwords_add");
    with_translated_string_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_add");
        let state = &*state;
        let mut parsed: serde_json::Value = serde_json::from_str(c_str_to_str(record_json))?;
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
    state: *const PasswordEngine,
    record_json: *const c_char,
    error: *mut ExternError
) {
    trace!("sync15_passwords_update");
    with_translated_void_result(error, || {
        assert!(!state.is_null(), "Null state passed to sync15_passwords_update");
        let state = &*state;
        let parsed: Login = serde_json::from_str(c_str_to_str(record_json))?;
        state.update(parsed)
    });
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_destroy_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[no_mangle]
pub unsafe extern "C" fn sync15_passwords_state_destroy(obj: *mut PasswordEngine) {
    if !obj.is_null() {
        drop(Box::from_raw(obj));
    }
}
