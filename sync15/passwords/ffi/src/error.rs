// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

use ffi_toolkit::string::{
    string_to_c_char
};
use std::ptr;
use std::os::raw::c_char;

use sync15_passwords::{
    Result,
};

pub unsafe fn with_translated_result<F, T>(error: *mut ExternError, callback: F) -> *mut T
where F: FnOnce() -> Result<T> {
    translate_result(callback(), error)
}

pub unsafe fn with_translated_void_result<F>(error: *mut ExternError, callback: F)
where F: FnOnce() -> Result<()> {
    translate_void_result(callback(), error);
}

pub unsafe fn with_translated_string_result<F>(error: *mut ExternError, callback: F) -> *mut c_char
where F: FnOnce() -> Result<String> {
    if let Some(s) = try_translate_result(callback(), error) {
        string_to_c_char(s)
    } else {
        ptr::null_mut()
    }
}

pub unsafe fn with_translated_opt_string_result<F>(error: *mut ExternError, callback: F) -> *mut c_char
where F: FnOnce() -> Result<Option<String>> {
    if let Some(Some(s)) = try_translate_result(callback(), error) {
        string_to_c_char(s)
    } else {
        // This is either an error case, or callback returned None.
        ptr::null_mut()
    }
}

// XXX rest of this is COPYPASTE from mentat/ffi/util.rs, this likely belongs in ffi-toolkit
// (something similar is there, but it is more error prone and not usable in a general way)
// XXX Actually, once errors are more stable we should do something more like fxa (and put that in
// ffi-toolkit). Yesterday I thought this was impossible but IDK I was tired? It's possible.

/// Represents an error that occurred on the mentat side. Many mentat FFI functions take a
/// `*mut ExternError` as the last argument. This is an out parameter that indicates an
/// error that occurred during that function's execution (if any).
///
/// For functions that use this pattern, if the ExternError's message property is null, then no
/// error occurred. If the message is non-null then it contains a string description of the
/// error that occurred.
///
/// Important: This message is allocated on the heap and it is the consumer's responsibility to
/// free it using `destroy_mentat_string`!
///
/// While this pattern is not ergonomic in Rust, it offers two main benefits:
///
/// 1. It avoids defining a large number of `Result`-shaped types in the FFI consumer, as would
///    be required with something like an `struct ExternResult<T> { ok: *mut T, err:... }`
/// 2. It offers additional type safety over `struct ExternResult { ok: *mut c_void, err:... }`,
///    which helps avoid memory safety errors.
#[repr(C)]
#[derive(Debug)]
pub struct ExternError {
    pub message: *mut c_char,
    // TODO: Include an error code here.
}

impl Default for ExternError {
    fn default() -> ExternError {
        ExternError { message: ptr::null_mut() }
    }
}
/// Translate Result<T, E>, into something C can understand, when T is not `#[repr(C)]`
///
/// - If `result` is `Ok(v)`, moves `v` to the heap and returns a pointer to it, and sets
///   `error` to a state indicating that no error occurred (`message` is null).
/// - If `result` is `Err(e)`, returns a null pointer and stores a string representing the error
///   message (which was allocated on the heap and should eventually be freed) into
///   `error.message`
pub unsafe fn translate_result<T>(result: Result<T>, error: *mut ExternError) -> *mut T {
    // TODO: can't unwind across FFI...
    assert!(!error.is_null(), "Error output parameter is not optional");
    let error = &mut *error;
    error.message = ptr::null_mut();
    match result {
        Ok(val) => Box::into_raw(Box::new(val)),
        Err(e) => {
            error!("Rust Error: {:?}", e);
            error.message = string_to_c_char(e.to_string());
            ptr::null_mut()
        }
    }
}

pub unsafe fn try_translate_result<T>(result: Result<T>, error: *mut ExternError) -> Option<T> {
    // TODO: can't unwind across FFI...
    assert!(!error.is_null(), "Error output parameter is not optional");
    let error = &mut *error;
    error.message = ptr::null_mut();
    match result {
        Ok(val) => Some(val),
        Err(e) => {
            error!("Rust Error: {:?}", e);
            error.message = string_to_c_char(e.to_string());
            None
        }
    }
}

/// Identical to `translate_result`, but with additional type checking for the case that we have
/// a `Result<(), E>` (which we're about to drop on the floor).
pub unsafe fn translate_void_result(result: Result<()>, error: *mut ExternError) {
    // TODO: update this comment.
    // Note that Box<T> guarantees that if T is zero sized, it's not heap allocated. So not
    // only do we never need to free the return value of this, it would be a problem if someone did.
    translate_result(result, error);
}
