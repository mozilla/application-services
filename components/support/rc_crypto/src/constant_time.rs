/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, util::ensure_nss_initialized};
use std::os::raw::c_void;

/// Returns `Ok(())` if `a == b` and `Error` otherwise.
/// The comparison of `a` and `b` is done in constant time with respect to the
/// contents of each, but NOT in constant time with respect to the lengths of
/// `a` and `b`.
pub fn verify_slices_are_equal(a: &[u8], b: &[u8]) -> Result<()> {
    // NSS_SecureMemcmp will compare N elements fron our slices,
    // so make sure they are the same length first.
    if a.len() != b.len() {
        return Err(ErrorKind::InternalError.into());
    }
    ensure_nss_initialized();
    let result = unsafe {
        nss_sys::NSS_SecureMemcmp(
            a.as_ptr() as *const c_void,
            b.as_ptr() as *const c_void,
            a.len(),
        )
    };
    match result {
        0 => Ok(()),
        _ => Err(ErrorKind::InternalError.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn does_compare() {
        assert!(verify_slices_are_equal(b"bobo", b"bobo").is_ok());
        assert!(verify_slices_are_equal(b"bobo", b"obob").is_err());
        assert!(verify_slices_are_equal(b"bobo", b"notbobo").is_err());
    }
}
