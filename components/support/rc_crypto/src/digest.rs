/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, util::ensure_nss_initialized, util::map_nss_secstatus};

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

impl From<&Algorithm> for nss_sys::SECOidTag::Type {
    fn from(alg: &Algorithm) -> Self {
        match alg {
            Algorithm::SHA256 => nss_sys::SECOidTag::SEC_OID_SHA256,
            // _ => nss_sys::SEC_OID_UNKNOWN,
        }
    }
}

pub fn digest(algorithm: &'static Algorithm, data: &[u8]) -> Result<Vec<u8>> {
    let mut out_buf = vec![0u8; algorithm.result_len()];
    ensure_nss_initialized();
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_HashBuf(
            algorithm.into(),
            out_buf.as_mut_ptr(),
            data.as_ptr(),
            data.len() as i32,
        )
    })?;
    Ok(out_buf)
}
