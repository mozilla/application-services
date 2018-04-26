use std::ffi::{CString, CStr};
use libc::c_char;

pub fn c_char_to_string(cchar: *const c_char) -> String {
    let c_str = unsafe { CStr::from_ptr(cchar) };
    let r_str = c_str.to_str().unwrap_or("");
    r_str.to_string()
}

pub fn string_to_c_char<T>(r_string: T) -> *mut c_char where T: Into<String> {
    CString::new(r_string.into()).unwrap().into_raw()
}