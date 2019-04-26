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

use crate::{constant_time, digest, error::*};
#[cfg(not(target_os = "ios"))]
use crate::{
    p11,
    util::{ensure_nss_initialized, map_nss_secstatus},
};
use std::convert::TryFrom;

/// A calculated signature value.
#[derive(Clone)]
pub struct Signature(digest::Digest);

impl AsRef<[u8]> for Signature {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// A key to use for HMAC signing.
pub struct SigningKey {
    pub(crate) digest_alg: &'static digest::Algorithm,
    pub(crate) key_value: Vec<u8>,
}

impl SigningKey {
    pub fn new(digest_alg: &'static digest::Algorithm, key_value: &[u8]) -> Self {
        SigningKey {
            digest_alg,
            key_value: key_value.to_vec(),
        }
    }

    #[inline]
    pub fn digest_algorithm(&self) -> &'static digest::Algorithm {
        self.digest_alg
    }
}

/// A key to use for HMAC authentication.
pub struct VerificationKey {
    wrapped: SigningKey,
}

impl VerificationKey {
    pub fn new(digest_alg: &'static digest::Algorithm, key_value: &[u8]) -> Self {
        VerificationKey {
            wrapped: SigningKey::new(digest_alg, key_value),
        }
    }

    #[inline]
    pub fn digest_algorithm(&self) -> &'static digest::Algorithm {
        self.wrapped.digest_algorithm()
    }
}

/// Calculate the HMAC of `data` using `key` and verify it correspond to the provided signature.
pub fn verify(key: &VerificationKey, data: &[u8], signature: &[u8]) -> Result<()> {
    verify_with_own_key(&key.wrapped, data, signature)
}

/// Equivalent to `verify` but allows the consumer to pass a `SigningKey`.
pub fn verify_with_own_key(key: &SigningKey, data: &[u8], signature: &[u8]) -> Result<()> {
    constant_time::verify_slices_are_equal(sign(key, data)?.as_ref(), signature)
}

/// Calculate the HMAC of `data` using `key`.
#[cfg(not(target_os = "ios"))]
pub fn sign(key: &SigningKey, data: &[u8]) -> Result<Signature> {
    let mech = match key.digest_alg {
        digest::Algorithm::SHA256 => nss_sys::CKM_SHA256_HMAC,
    };
    ensure_nss_initialized();
    let sym_key = p11::import_sym_key(mech.into(), nss_sys::CKA_SIGN.into(), &key.key_value)?;
    let context = p11::create_context_by_sym_key(mech.into(), nss_sys::CKA_SIGN.into(), &sym_key)?;
    map_nss_secstatus(|| unsafe { nss_sys::PK11_DigestBegin(context.as_mut_ptr()) })?;
    let data_len = u32::try_from(data.len())?;
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_DigestOp(context.as_mut_ptr(), data.as_ptr(), data_len)
    })?;
    // We allocate the maximum possible length for the out buffer then we'll
    // slice it after nss fills `out_len`.
    let mut out_len: u32 = 0;
    let mut out = vec![0u8; nss_sys::HASH_LENGTH_MAX as usize];
    map_nss_secstatus(|| unsafe {
        nss_sys::PK11_DigestFinal(
            context.as_mut_ptr(),
            out.as_mut_ptr(),
            &mut out_len,
            nss_sys::HASH_LENGTH_MAX,
        )
    })?;
    out.truncate(usize::try_from(out_len)?);
    Ok(Signature(digest::Digest {
        value: out,
        algorithm: key.digest_alg,
    }))
}

#[cfg(target_os = "ios")]
pub fn sign(key: &SigningKey, data: &[u8]) -> Result<Signature> {
    let ring_digest = match key.digest_alg {
        digest::Algorithm::SHA256 => &ring::digest::SHA256,
    };
    let ring_key = ring::hmac::SigningKey::new(&ring_digest, &key.key_value);
    let ring_signature = ring::hmac::sign(&ring_key, data);
    Ok(Signature(digest::Digest {
        value: ring_signature.as_ref().to_vec(),
        algorithm: key.digest_alg,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;
    #[test]
    fn hmac_sign_verify() {
        let key = VerificationKey::new(&digest::SHA256, b"key");
        let expected_signature =
            hex::decode("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8")
                .unwrap();
        assert!(verify(
            &key,
            b"The quick brown fox jumps over the lazy dog",
            &expected_signature
        )
        .is_ok());
    }
}
