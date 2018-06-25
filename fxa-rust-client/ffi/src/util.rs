/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use libc::c_char;
use std::ffi::{CStr, CString};

pub unsafe fn c_char_to_string(cchar: *const c_char) -> &'static str {
    assert!(!cchar.is_null(), "Null pointer passed to rust!");
    let c_str = CStr::from_ptr(cchar);
    c_str.to_str().unwrap_or("")
}

pub fn string_to_c_char<T>(r_string: T) -> *mut c_char
where
    T: Into<String>,
{
    CString::new(r_string.into()).unwrap().into_raw()
}
