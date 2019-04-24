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
#[cfg(not(target_os = "ios"))]
use crate::util::ensure_nss_initialized;
use std::os::raw::c_void;

/// Returns `Ok(())` if `a == b` and `Error` otherwise.
/// The comparison of `a` and `b` is done in constant time with respect to the
/// contents of each, but NOT in constant time with respect to the lengths of
/// `a` and `b`.
#[cfg(not(target_os = "ios"))]
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
#[cfg(target_os = "ios")]
pub fn verify_slices_are_equal(a: &[u8], b: &[u8]) -> Result<()> {
    ring::constant_time::verify_slices_are_equal(a, b).map_err(|_| ErrorKind::InternalError.into())
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
