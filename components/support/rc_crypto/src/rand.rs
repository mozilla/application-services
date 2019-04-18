/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

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
