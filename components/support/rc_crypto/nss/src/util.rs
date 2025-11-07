/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use nss_sys::*;
use std::{ffi::CString, os::raw::c_char};

use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(feature = "keydb")]
use crate::pk11::slot;

// This is the NSS version that this crate is claiming to be compatible with.
// We check it at runtime using `NSS_VersionCheck`.
pub const COMPATIBLE_NSS_VERSION: &str = "3.26";

// Expect NSS has been initialized. This is usually be done via `init_rust_components`, see
// components/init_rust_components/README.md.
pub fn assert_nss_initialized() {
    INITIALIZED.get().expect(
        "NSS has not initialized.
    Please ensure you include the initialization component and call it early in your code. See
    https://mozilla.github.io/application-services/book/rust-docs/init_rust_components/index.html",
    );
}

// This and many other nss init code were either taken directly from or are inspired by
// https://github.com/mozilla/neqo/blob/b931a289eee7d62c0815535f01cfa34c5a929f9d/neqo-crypto/src/lib.rs#L73-L77
enum NssLoaded {
    External,
    NoDb,
    #[cfg(feature = "keydb")]
    Db,
}

static INITIALIZED: OnceLock<NssLoaded> = OnceLock::new();

fn assert_compatible_version() {
    let min_ver = CString::new(COMPATIBLE_NSS_VERSION).unwrap();
    if unsafe { NSS_VersionCheck(min_ver.as_ptr()) } == PR_FALSE {
        panic!("Incompatible NSS version!")
    }
}

fn init_once(profile_path: Option<PathBuf>) -> NssLoaded {
    assert_compatible_version();

    if unsafe { NSS_IsInitialized() != PR_FALSE } {
        return NssLoaded::External;
    }

    match profile_path {
        #[cfg(feature = "keydb")]
        Some(path) => {
            if !path.is_dir() {
                panic!("missing profile directory {:?}", path);
            }
            let pathstr = path.to_str().expect("invalid path");
            let dircstr = CString::new(pathstr).expect("could not build CString from path");
            let empty = CString::new("").expect("could not build empty CString");
            let flags = NSS_INIT_FORCEOPEN | NSS_INIT_OPTIMIZESPACE;

            let context = unsafe {
                NSS_InitContext(
                    dircstr.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr(),
                    std::ptr::null_mut(),
                    flags,
                )
            };
            if context.is_null() {
                let error = get_last_error();
                panic!("could not initialize context: {}", error);
            }

            let slot = slot::get_internal_key_slot().expect("could not get internal key slot");

            if unsafe { PK11_NeedUserInit(slot.as_mut_ptr()) } == nss_sys::PR_TRUE {
                let result = unsafe {
                    PK11_InitPin(
                        slot.as_mut_ptr(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                if result != SECStatus::SECSuccess {
                    let error = get_last_error();
                    panic!("could not initialize context: {}", error);
                }
            }

            NssLoaded::Db
        }

        #[cfg(not(feature = "keydb"))]
        Some(_) => panic!("Use the keydb feature to enable nss initialization with profile path"),

        None => {
            let empty = CString::default();
            let flags = NSS_INIT_READONLY
                | NSS_INIT_NOCERTDB
                | NSS_INIT_NOMODDB
                | NSS_INIT_FORCEOPEN
                | NSS_INIT_OPTIMIZESPACE;
            let context = unsafe {
                NSS_InitContext(
                    empty.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr(),
                    empty.as_ptr(),
                    std::ptr::null_mut(),
                    flags,
                )
            };
            if context.is_null() {
                let error = get_last_error();
                panic!("Could not initialize NSS: {}", error);
            }

            NssLoaded::NoDb
        }
    }
}

/// Initialize NSS. This only executes the initialization routines once, so if there is any chance
/// that this is invoked twice, that's OK.
///
/// # Errors
///
/// When NSS initialization fails.
pub fn ensure_nss_initialized() {
    INITIALIZED.get_or_init(|| init_once(None));
}

/// Use this function to initialize NSS if you want to manage keys with NSS.
/// ensure_initialized_with_profile_dir initializes NSS with a profile directory (where key4.db
/// will be stored) and appropriate flags to persist keys (and certificates) in its internal PKCS11
/// software implementation.
/// If it has been called previously with a different path, it will fail.
/// If `ensure_initialized` has been called before, it will also fail.
#[cfg(feature = "keydb")]
pub fn ensure_nss_initialized_with_profile_dir<P: Into<PathBuf>>(dir: P) {
    INITIALIZED.get_or_init(|| init_once(Some(dir.into())));
}

pub fn map_nss_secstatus<F>(callback: F) -> Result<()>
where
    F: FnOnce() -> SECStatus,
{
    if callback() == SECStatus::SECSuccess {
        return Ok(());
    }
    Err(get_last_error())
}

/// Retrieve and wrap the last NSS/NSPR error in the current thread.
#[cold]
pub fn get_last_error() -> Error {
    let error_code = unsafe { PR_GetError() };
    let error_text: String = usize::try_from(unsafe { PR_GetErrorTextLength() })
        .map(|error_text_len| {
            let mut out_str = vec![0u8; error_text_len + 1];
            unsafe { PR_GetErrorText(out_str.as_mut_ptr() as *mut c_char) };
            CString::new(&out_str[0..error_text_len])
                .unwrap_or_else(|_| CString::default())
                .to_str()
                .unwrap_or("")
                .to_owned()
        })
        .unwrap_or_else(|_| "".to_string());
    ErrorKind::NSSError(error_code, error_text).into()
}

pub(crate) trait ScopedPtr
where
    Self: std::marker::Sized,
{
    type RawType;
    unsafe fn from_ptr(ptr: *mut Self::RawType) -> Result<Self>;
    fn as_ptr(&self) -> *const Self::RawType;
    fn as_mut_ptr(&self) -> *mut Self::RawType;
}

// The macro defines a wrapper around pointers referring to types allocated by NSS,
// calling their NSS destructor method when they go out of scope to avoid memory leaks.
// The `as_ptr`/`as_mut_ptr` are provided to retrieve the raw pointers to pass to
// NSS functions that consume them.
#[macro_export]
macro_rules! scoped_ptr {
    ($scoped:ident, $target:ty, $dtor:path) => {
        pub struct $scoped {
            ptr: *mut $target,
        }

        impl $crate::util::ScopedPtr for $scoped {
            type RawType = $target;

            #[allow(dead_code)]
            unsafe fn from_ptr(ptr: *mut $target) -> $crate::error::Result<$scoped> {
                if !ptr.is_null() {
                    Ok($scoped { ptr })
                } else {
                    Err($crate::error::ErrorKind::InternalError.into())
                }
            }

            #[inline]
            fn as_ptr(&self) -> *const $target {
                self.ptr
            }

            #[inline]
            fn as_mut_ptr(&self) -> *mut $target {
                self.ptr
            }
        }

        impl Drop for $scoped {
            fn drop(&mut self) {
                assert!(!self.ptr.is_null());
                unsafe { $dtor(self.ptr) };
            }
        }
    };
}

/// Copies a SECItem into a slice
///
/// # Safety
///
/// The returned reference must not outlive the `sym_key`, since that owns the `SecItem` buffer.
pub(crate) unsafe fn sec_item_as_slice(sec_item: &mut SECItem) -> Result<&mut [u8]> {
    let sec_item_buf_len = usize::try_from(sec_item.len)?;
    let buf = std::slice::from_raw_parts_mut(sec_item.data, sec_item_buf_len);
    Ok(buf)
}

#[cfg(not(feature = "keydb"))]
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_assert_initialized() {
        ensure_nss_initialized();
        assert_nss_initialized();
    }

    #[test]
    fn test_ensure_initialized_multithread() {
        let threads: Vec<_> = (0..2)
            .map(|_| thread::spawn(ensure_nss_initialized))
            .collect();

        for handle in threads {
            handle.join().unwrap();
        }
    }
}

#[cfg(feature = "keydb")]
#[cfg(test)]
mod tests_keydb {
    use super::*;
    use std::thread;

    fn profile_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/profile")
    }

    #[test]
    fn test_assert_initialized_with_profile_dir() {
        ensure_nss_initialized_with_profile_dir(profile_path());
        assert_nss_initialized();
    }

    #[test]
    fn test_ensure_initialized_with_profile_dir_multithread() {
        let threads: Vec<_> = (0..2)
            .map(|_| thread::spawn(move || ensure_nss_initialized_with_profile_dir(profile_path())))
            .collect();

        for handle in threads {
            handle.join().unwrap();
        }
    }
}
