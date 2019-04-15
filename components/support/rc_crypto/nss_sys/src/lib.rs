/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

use std::os::raw::{c_char, c_uchar};

include!(concat!(env!("OUT_DIR"), "/nss_bindings.rs"));

// Remap some constants.
pub const SECSuccess: SECStatus = _SECStatus_SECSuccess;
pub const SECFailure: SECStatus = _SECStatus_SECFailure;
pub const PR_FALSE: PRBool = 0;
pub const PR_TRUE: PRBool = 1;

// This is the version this crate is claiming to be compatible with.
// We check it at runtime using `NSS_VersionCheck`.
pub const COMPATIBLE_NSS_VERSION: &str = "3.26";

// Code adapted from https://stackoverflow.com/a/35591693. I am not this kind of smart.
macro_rules! nss_exports {
    () => {};
    (
        unsafe fn $fn_name:ident($($arg:ident: $argty:ty),*) -> $ret:ty;
        $($tail:tt)*
    ) => {
        #[cfg(not(target_os = "ios"))]
        lazy_static::lazy_static! {
            pub static ref $fn_name: libloading::Symbol<'static, unsafe extern fn($($arg: $argty),*) -> $ret> = {
                unsafe {
                    LIBNSS3.get(stringify!($fn_name).as_bytes()).expect(stringify!(Could not get $fn_name handle))
                }
            };
        }
        #[cfg(target_os = "ios")]
        extern "C" {
            pub fn $fn_name($($arg: $argty),*) -> $ret;
        }
        nss_exports! { $($tail)* }
    };
    // Support for functions that don't return can be added by copy-pasting the above code and removing -> ref:ty.
}

#[cfg(not(target_os = "ios"))]
lazy_static::lazy_static! {
    // Lib handle.
    static ref LIBNSS3: libloading::Library = {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        const LIB_NAME: &str = "libnss3.dylib";
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const LIB_NAME: &str = "libnss3.so";
        #[cfg(target_os = "windows")]
        const LIB_NAME: &str = "nss3.dll";
        libloading::Library::new(LIB_NAME).expect("Cannot load libnss3.")
    };
}

nss_exports! {
    unsafe fn PR_GetError() -> PRErrorCode;
    unsafe fn PR_GetErrorTextLength() -> PRInt32;
    unsafe fn PR_GetErrorText(out: *mut c_uchar) -> PRInt32;
    unsafe fn NSS_NoDB_Init(configdir: *const c_char) -> SECStatus;
    unsafe fn NSS_InitContext(configdir: *const c_char, certPrefix: *const c_char, keyPrefix: *const c_char, secmodName: *const c_char, initParams: *mut NSSInitParameters, flags: PRUint32) -> *mut NSSInitContext;
    unsafe fn NSS_IsInitialized() -> PRBool;
    unsafe fn NSS_GetVersion() -> *const c_char;
    unsafe fn NSS_VersionCheck(importedVersion: *const c_char) -> PRBool;
    unsafe fn PK11_HashBuf(hashAlg: SECOidTag::Type, out: *mut c_uchar, r#in: *const c_uchar, len: PRInt32) -> SECStatus;
}
