/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter as sync;
extern crate libc;

use sync::record_types::{PasswordRecord};
use std::ffi::{CStr, CString};
use libc::c_char;
use std::ptr;

fn c_char_to_string(cchar: *const c_char) -> String {
    let c_str = unsafe { CStr::from_ptr(cchar) };
    let r_str = c_str.to_str().unwrap_or("");
    r_str.to_string()
}

fn string_to_c_char(s: String) -> *mut c_char {
    CString::new(s).unwrap().into_raw()
}

fn opt_string_to_c_char(os: Option<String>) -> *mut c_char {
    match os {
        Some(s) => string_to_c_char(s),
        _ => ptr::null_mut(),
    }
}

#[repr(C)]
pub struct PasswordRecordC {
    pub id: *mut c_char,

    /// Might be null!
    pub hostname: *mut c_char,

    /// Might be null!
    pub form_submit_url: *mut c_char,
    pub http_realm: *mut c_char,

    pub username: *mut c_char,
    pub password: *mut c_char,

    pub username_field: *mut c_char,
    pub password_field: *mut c_char,

    /// In ms since unix epoch
    pub time_created: i64,

    /// In ms since unix epoch
    pub time_password_changed: i64,

    /// -1 for missing, otherwise in ms_since_unix_epoch
    pub time_last_used: i64,

    /// -1 for missing
    pub times_used: i64,
}

unsafe fn drop_cchar_ptr(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

impl Drop for PasswordRecordC {
    fn drop(&mut self) {
        unsafe {
            drop_cchar_ptr(self.id);
            drop_cchar_ptr(self.hostname);
            drop_cchar_ptr(self.form_submit_url);
            drop_cchar_ptr(self.http_realm);
            drop_cchar_ptr(self.username);
            drop_cchar_ptr(self.password);
            drop_cchar_ptr(self.username_field);
            drop_cchar_ptr(self.password_field);
        }
    }
}

impl From<PasswordRecord> for PasswordRecordC {
    fn from(record: PasswordRecord) -> PasswordRecordC {
        PasswordRecordC {
            id: string_to_c_char(record.id),
            hostname: opt_string_to_c_char(record.hostname),
            form_submit_url: opt_string_to_c_char(record.form_submit_url),
            http_realm: opt_string_to_c_char(record.http_realm),
            username: string_to_c_char(record.username),
            password: string_to_c_char(record.password),
            username_field: string_to_c_char(record.username_field),
            password_field: string_to_c_char(record.password_field),
            time_created: record.time_created,
            time_password_changed: record.time_password_changed,
            time_last_used: record.time_last_used.unwrap_or(-1),
            times_used: record.times_used.unwrap_or(-1),
        }
    }
}

// This is opaque to C
pub struct PasswordCollection {
    pub records: Vec<PasswordRecord>,
    pub tombstones: Vec<String>,
}

#[no_mangle]
pub extern "C" fn sync15_passwords_destroy(coll: *mut PasswordCollection) {
    let _ = unsafe { Box::from_raw(coll) };
}

#[no_mangle]
pub extern "C" fn sync15_passwords_tombstone_count(coll: *const PasswordCollection) -> libc::size_t {
    let coll = unsafe { &*coll };
    coll.tombstones.len() as libc::size_t
}

#[no_mangle]
pub extern "C" fn sync15_passwords_record_count(coll: *const PasswordCollection) -> libc::size_t {
    let coll = unsafe { &*coll };
    coll.records.len() as libc::size_t
}

#[no_mangle]
pub extern "C" fn sync15_passwords_get_tombstone_at(
    coll: *const PasswordCollection,
    index: libc::size_t
) -> *mut c_char {
    let coll = unsafe { &*coll };
    opt_string_to_c_char(coll.tombstones.get(index as usize).cloned())
}

#[no_mangle]
pub extern "C" fn sync15_passwords_get_record_at(
    coll: *const PasswordCollection,
    index: libc::size_t
) -> *mut PasswordRecordC {
    let coll = unsafe { &*coll };
    match coll.records.get(index as usize) {
        Some(r) => Box::into_raw(Box::new(r.clone().into())),
        None => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn sync15_password_record_destroy(pw: *mut PasswordRecordC) {
    // Our drop impl takes care of our strings.
    let _ = unsafe { Box::from_raw(pw) };
}
