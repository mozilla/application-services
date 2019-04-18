/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{digest, error::*, hmac};
#[cfg(not(target_os = "ios"))]
use crate::{
    p11,
    util::{ensure_nss_initialized, map_nss_secstatus},
};
#[cfg(not(target_os = "ios"))]
use nss_sys::*;
use std::{
    convert::TryFrom,
    mem,
    os::raw::{c_uchar, c_ulong},
    ptr,
};

pub fn extract_and_expand(
    salt: &hmac::SigningKey,
    secret: &[u8],
    info: &[u8],
    out: &mut [u8],
) -> Result<()> {
    let prk = extract(salt, secret)?;
    expand(&prk, info, out)?;
    Ok(())
}

pub fn extract(salt: &hmac::SigningKey, secret: &[u8]) -> Result<hmac::SigningKey> {
    let prk = hmac::sign(salt, secret)?;
    Ok(hmac::SigningKey::new(salt.digest_algorithm(), prk.as_ref()))
}

#[cfg(target_os = "ios")]
pub fn expand(prk: &hmac::SigningKey, info: &[u8], out: &mut [u8]) -> Result<()> {
    let ring_digest = match prk.digest_alg {
        digest::Algorithm::SHA256 => &ring::digest::SHA256,
    };
    let ring_prk = ring::hmac::SigningKey::new(&ring_digest, &prk.key_value);
    ring::hkdf::expand(&ring_prk, info, out);
    Ok(())
}

#[cfg(not(target_os = "ios"))]
pub fn expand(prk: &hmac::SigningKey, info: &[u8], out: &mut [u8]) -> Result<()> {
    let mech = match prk.digest_algorithm() {
        digest::Algorithm::SHA256 => CKM_NSS_HKDF_SHA256,
    };
    ensure_nss_initialized();
    // Most of the following code is inspired by the Firefox WebCrypto implementation:
    // https://searchfox.org/mozilla-central/rev/ee3905439acbf81e9c829ece0b46d09d2fa26c5c/dom/crypto/WebCryptoTask.cpp#2530-2597
    // Except that we only do the expand part, which explains why we use null pointers bellow.
    let mut hkdf_params = CK_NSS_HKDFParams {
        bExtract: CK_FALSE,
        pSalt: ptr::null_mut(),
        ulSaltLen: 0,
        bExpand: CK_TRUE,
        pInfo: info.as_ptr() as *mut u8,
        ulInfoLen: c_ulong::try_from(info.len())?,
    };
    let mut params = SECItem {
        type_: SECItemType::siBuffer,
        data: &mut hkdf_params as *mut _ as *mut c_uchar,
        len: u32::try_from(mem::size_of::<CK_NSS_HKDFParams>())?,
    };
    let base_key = p11::import_sym_key(mech.into(), CKA_WRAP.into(), &prk.key_value)?;
    let len = i32::try_from(out.len())?;
    let sym_key = p11::SymKey::from_ptr(unsafe {
        // CKM_SHA512_HMAC and CKA_SIGN are key type and usage attributes of the
        // derived symmetric key and don't matter because we ignore them anyway.
        PK11_Derive(
            base_key.as_mut_ptr(),
            mech.into(),
            &mut params,
            CKM_SHA512_HMAC.into(),
            CKA_SIGN.into(),
            len,
        )
    })?;

    map_nss_secstatus(|| unsafe { PK11_ExtractKeyValue(sym_key.as_mut_ptr()) })?;

    // This doesn't leak, because the SECItem* returned by PK11_GetKeyData
    // just refers to a buffer managed by `symKey` which we copy into `out`.
    let key_data = unsafe { *PK11_GetKeyData(sym_key.as_mut_ptr()) };
    if u32::try_from(out.len())? > key_data.len {
        return Err(ErrorKind::InternalError.into());
    }
    let key_data_len = usize::try_from(key_data.len)?;
    let buf = unsafe { std::slice::from_raw_parts(key_data.data, key_data_len) };
    out.copy_from_slice(&buf[0..out.len()]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;
    #[test]
    fn hkdf_extract_expand() {
        let secret = hex::decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b").unwrap();
        let salt = hex::decode("000102030405060708090a0b0c").unwrap();
        let info = hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap();
        let expected_out = hex::decode(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        )
        .unwrap();
        let salt = hmac::SigningKey::new(&digest::SHA256, &salt);
        let mut out = vec![0u8; expected_out.len()];
        extract_and_expand(&salt, &secret, &info, &mut out).unwrap();
        assert_eq!(out, expected_out);
    }
}
