/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
#[cfg(not(target_os = "ios"))]
use crate::util::{ensure_nss_initialized, map_nss_secstatus};
use std::convert::TryFrom;

pub enum Algorithm {
    SHA256,
}
pub use Algorithm::*;

impl Algorithm {
    fn result_len(&self) -> usize {
        match self {
            Algorithm::SHA256 => 32,
        }
    }
}

#[cfg(not(target_os = "ios"))]
impl From<&Algorithm> for nss_sys::SECOidTag::Type {
    fn from(alg: &Algorithm) -> Self {
        match alg {
            Algorithm::SHA256 => nss_sys::SECOidTag::SEC_OID_SHA256,
        }
    }
}

/// A calculated digest value.
#[derive(Clone)]
pub struct Digest {
    pub(crate) value: Vec<u8>,
    pub(crate) algorithm: &'static Algorithm,
}

impl Digest {
    pub fn algorithm(&self) -> &'static Algorithm {
        self.algorithm
    }
}

impl AsRef<[u8]> for Digest {
    fn as_ref(&self) -> &[u8] {
        self.value.as_ref()
    }
}

/// Returns the digest of data using the given digest algorithm.
#[cfg(not(target_os = "ios"))]
pub fn digest(algorithm: &'static Algorithm, data: &[u8]) -> Result<Digest> {
    let mut out_buf = vec![0u8; algorithm.result_len()];
    ensure_nss_initialized();
    let data_len = i32::try_from(data.len())?;
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_HashBuf(
            algorithm.into(),
            out_buf.as_mut_ptr(),
            data.as_ptr(),
            data_len,
        )
    })?;
    Ok(Digest {
        value: out_buf,
        algorithm,
    })
}

#[cfg(target_os = "ios")]
pub fn digest(algorithm: &'static Algorithm, data: &[u8]) -> Result<Digest> {
    let ring_alg = match algorithm {
        Algorithm::SHA256 => &ring::digest::SHA256,
    };
    let ring_digest = ring::digest::digest(&ring_alg, data);
    Ok(Digest {
        value: ring_digest.as_ref().to_vec(),
        algorithm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;
    #[test]
    fn sha256_digest() {
        assert_eq!(
            hex::encode(&digest(&SHA256, b"bobo").unwrap()),
            "bf0c97708b849de696e7373508b13c5ea92bafa972fc941d694443e494a4b84d"
        );
    }
}
