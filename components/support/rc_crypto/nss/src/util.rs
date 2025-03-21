/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use nss_sys::*;
use std::{ffi::CString, os::raw::c_char, sync::Once};

#[cfg(feature = "keydb")]
use crate::pk11::slot;
#[cfg(feature = "keydb")]
use once_cell::sync::OnceCell;
#[cfg(feature = "keydb")]
use std::{fs, path::Path};

// This is the NSS version that this crate is claiming to be compatible with.
// We check it at runtime using `NSS_VersionCheck`.
pub const COMPATIBLE_NSS_VERSION: &str = "3.26";

static NSS_INIT: Once = Once::new();
#[cfg(feature = "keydb")]
static NSS_PROFILE_PATH: OnceCell<String> = OnceCell::new();

pub fn ensure_nss_initialized() {
    NSS_INIT.call_once(|| {
        let version_ptr = CString::new(COMPATIBLE_NSS_VERSION).unwrap();
        if unsafe { NSS_VersionCheck(version_ptr.as_ptr()) == PR_FALSE } {
            panic!("Incompatible NSS version!")
        }
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
    })
}

/// Use this function to initialize NSS if you want to manage keys with NSS.
/// ensure_initialized_with_profile_dir initializes NSS with a profile directory (where key4.db
/// will be stored) and appropriate flags to persist keys (and certificates) in its internal PKCS11
/// software implementation.
/// If it has been called previously with a different path, it will fail.
/// If `ensure_initialized` has been called before, it will also fail.
#[cfg(feature = "keydb")]
pub fn ensure_nss_initialized_with_profile_dir(path: impl AsRef<Path>) -> Result<()> {
    match path.as_ref().to_str() {
        Some(path) => {
            if let Some(old_path) = NSS_PROFILE_PATH.get() {
                if old_path == path {
                    return Ok(());
                } else {
                    return Err(ErrorKind::NSSInitFailure(format!(
                        "already initialized with profile: {}",
                        old_path
                    ))
                    .into());
                }
            }
        }
        None => {
            return Err(ErrorKind::NSSInitFailure(format!(
                "invalid profile path: {:?}",
                path.as_ref()
            ))
            .into());
        }
    }

    if NSS_INIT.is_completed() {
        return Err(ErrorKind::NSSInitFailure(
            "NSS has been already initialized without profile".to_string(),
        )
        .into());
    }

    let version_ptr = CString::new(COMPATIBLE_NSS_VERSION).unwrap();
    if unsafe { NSS_VersionCheck(version_ptr.as_ptr()) == PR_FALSE } {
        panic!("Incompatible NSS version!")
    }

    if fs::metadata(path.as_ref()).is_err() {
        return Err(ErrorKind::NSSInitFailure(format!(
            "invalid profile path: {:?}",
            path.as_ref()
        ))
        .into());
    }

    // path must be valid unicode at this point because we just checked its metadata
    let c_path: CString =
        CString::new(path.as_ref().to_str().unwrap()).map_err(|_| ErrorKind::NulError)?;
    let empty = CString::default();
    let flags = NSS_INIT_FORCEOPEN | NSS_INIT_OPTIMIZESPACE;

    let context = unsafe {
        NSS_InitContext(
            c_path.as_ptr(),
            empty.as_ptr(),
            empty.as_ptr(),
            empty.as_ptr(),
            std::ptr::null_mut(),
            flags,
        )
    };
    if context.is_null() {
        let error = get_last_error();
        return Err(
            ErrorKind::NSSInitFailure(format!("could not initialize context: {}", error)).into(),
        );
    }

    let slot = slot::get_internal_key_slot().map_err(|error| {
        ErrorKind::NSSInitFailure(format!("could not get internal key slot: {}", error))
    })?;

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
            return Err(
                ErrorKind::NSSInitFailure(format!("could not initialize pin: {}", error)).into(),
            );
        }
    }

    NSS_PROFILE_PATH
        .set(format!("{:?}", path.as_ref()))
        .map_err(|error| ErrorKind::NSSInitFailure(format!("already initialized: {}", error)))?;

    Ok(())
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

#[cfg(test)]
#[cfg(feature = "keydb")]
mod test {
    use super::*;

    #[test]
    #[should_panic]
    fn test_ensure_nss_initialized_with_profile_dir_with_previously_call_to_ensure_nss_initialized()
    {
        ensure_nss_initialized();
        ensure_nss_initialized_with_profile_dir("./").unwrap();
    }
}
