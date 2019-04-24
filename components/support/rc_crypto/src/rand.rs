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
use crate::util::{ensure_nss_initialized, map_nss_secstatus};
use std::convert::TryFrom;

/// Fill a buffer with cryptographically secure pseudo-random data.
#[cfg(not(target_os = "ios"))]
pub fn fill(dest: &mut [u8]) -> Result<()> {
    // `NSS_Init` will initialize the RNG with data from `/dev/urandom`.
    ensure_nss_initialized();
    let len = i32::try_from(dest.len())?;
    map_nss_secstatus(|| unsafe { nss_sys::PK11_GenerateRandom(dest.as_mut_ptr(), len) })?;
    Ok(())
}

#[cfg(target_os = "ios")]
pub fn fill(dest: &mut [u8]) -> Result<()> {
    use ring::rand::SecureRandom;
    let rng = ring::rand::SystemRandom::new();
    rng.fill(dest).map_err(|_| ErrorKind::InternalError.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn random_fill() {
        let mut out = vec![0u8; 64];
        assert!(fill(&mut out).is_ok());
        assert_ne!(out, vec![0u8; 64]);
    }
}
