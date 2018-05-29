use libc::c_char;
use std::ffi::{CStr, CString};

pub fn c_char_to_string(cchar: *const c_char) -> &'static str {
    let c_str = unsafe { CStr::from_ptr(cchar) };
    c_str.to_str().unwrap_or("")
}

pub fn string_to_c_char<T>(r_string: T) -> *mut c_char
where
    T: Into<String>,
{
    CString::new(r_string.into()).unwrap().into_raw()
}
