/* Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY
 * SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
 * OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
 * CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE. */

use crate::error::*;
use nss_sys::*;
use std::{convert::TryFrom, ffi::CString, sync::Once};

static NSS_INIT: Once = Once::new();

pub fn ensure_nss_initialized() {
    NSS_INIT.call_once(|| {
        let version_ptr = CString::new(nss_sys::COMPATIBLE_NSS_VERSION).unwrap();
        if unsafe { NSS_VersionCheck(version_ptr.as_ptr()) == PR_FALSE } {
            panic!("Incompatible NSS version!")
        }
        if unsafe { NSS_IsInitialized() } == PR_FALSE {
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
        }
    })
}

pub fn map_nss_secstatus<F>(callback: F) -> Result<()>
where
    F: FnOnce() -> SECStatus,
{
    if callback() == SECSuccess {
        return Ok(());
    }
    Err(get_last_error())
}

/// Retrieve and wrap the last NSS/NSPR error in the current thread.
pub fn get_last_error() -> Error {
    let error_code = unsafe { PR_GetError() };
    let error_text: String = usize::try_from(unsafe { PR_GetErrorTextLength() })
        .map(|error_text_len| {
            let mut out_str = vec![0u8; error_text_len + 1];
            unsafe { PR_GetErrorText(out_str.as_mut_ptr()) };
            CString::new(&out_str[0..error_text_len])
                .unwrap_or_else(|_| CString::default())
                .to_str()
                .unwrap_or_else(|_| "")
                .to_owned()
        })
        .unwrap_or_else(|_| "".to_string());
    ErrorKind::NSSError(error_code, error_text).into()
}
