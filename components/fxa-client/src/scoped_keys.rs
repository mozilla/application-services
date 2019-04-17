/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::errors::*;
use byteorder::{BigEndian, ByteOrder};
use rc_crypto::digest;
use ring::{aead, agreement, agreement::EphemeralPrivateKey, rand::SecureRandom};
use serde_json::{self, json};
use untrusted::Input;

pub struct ScopedKeysFlow {
    private_key: EphemeralPrivateKey,
}

/// Theorically, everything done in this file could and should be done in a JWT library.
/// However, none of the existing rust JWT libraries can handle ECDH-ES encryption, and API choices
/// made by their authors make it difficult to add this feature.
/// In the past, we chose cjose to do that job, but it added three C dependencies to build and link
/// against: jansson, openssl and cjose itself.
impl ScopedKeysFlow {
    pub fn with_random_key(rng: &dyn SecureRandom) -> Result<ScopedKeysFlow> {
        let private_key = EphemeralPrivateKey::generate(&agreement::ECDH_P256, rng)
            .map_err(|_| ErrorKind::KeyGenerationFailed)?;
        Ok(ScopedKeysFlow { private_key })
    }

    pub fn generate_keys_jwk(&self) -> Result<String> {
        let pub_key = &self
            .private_key
            .compute_public_key()
            .map_err(|_| ErrorKind::PublicKeyComputationFailed)?;
        let pub_key_bytes = pub_key.as_ref();
        // Uncompressed form (see SECG SEC1 section 2.3.3).
        // First byte is 4, then 32 bytes for x, and 32 bytes for y.
        assert_eq!(pub_key_bytes.len(), 1 + 32 + 32);
        assert_eq!(pub_key_bytes[0], 0x04);
        let x = Vec::from(&pub_key_bytes[1..33]);
        let x = base64::encode_config(&x, base64::URL_SAFE_NO_PAD);
        let y = Vec::from(&pub_key_bytes[33..]);
        let y = base64::encode_config(&y, base64::URL_SAFE_NO_PAD);
        Ok(json!({
            "crv": "P-256",
            "kty": "EC",
            "x": x,
            "y": y,
        })
        .to_string())
    }

    pub fn decrypt_keys_jwe(self, jwe: &str) -> Result<String> {
        let segments: Vec<&str> = jwe.split('.').collect();
        let header = base64::decode_config(&segments[0], base64::URL_SAFE_NO_PAD)?;
        let protected_header: serde_json::Value = serde_json::from_slice(&header)?;
        assert_eq!(protected_header["epk"]["kty"], "EC");
        assert_eq!(protected_header["epk"]["crv"], "P-256");

        // Part 1: Grab the x/y from the other party and construct the secret.
        let x = base64::decode_config(
            &protected_header["epk"]["x"].as_str().unwrap(),
            base64::URL_SAFE_NO_PAD,
        )?;
        let y = base64::decode_config(
            &protected_header["epk"]["y"].as_str().unwrap(),
            base64::URL_SAFE_NO_PAD,
        )?;
        assert_eq!(x.len(), 256 / 8);
        assert_eq!(y.len(), 256 / 8);
        let mut peer_pub_key: Vec<u8> = vec![0x04];
        peer_pub_key.extend_from_slice(&x);
        peer_pub_key.extend_from_slice(&y);
        let peer_pub_key = Input::from(&peer_pub_key);
        let secret = agreement::agree_ephemeral(
            self.private_key,
            &agreement::ECDH_P256,
            peer_pub_key,
            ErrorKind::KeyAgreementFailed,
            |z| {
                // ConcatKDF (1 iteration since keyLen <= hashLen).
                // See rfc7518 section 4.6 for reference.
                let counter = 1;
                let alg = protected_header["enc"].as_str().unwrap();
                let apu = protected_header["apu"].as_str().unwrap_or("");
                let apv = protected_header["apv"].as_str().unwrap_or("");
                let mut buf: Vec<u8> = vec![];
                buf.extend_from_slice(&to_32b_buf(counter));
                buf.extend_from_slice(&z);
                // otherinfo
                buf.extend_from_slice(&to_32b_buf(alg.len() as u32));
                buf.extend_from_slice(alg.as_bytes());
                buf.extend_from_slice(&to_32b_buf(apu.len() as u32));
                buf.extend_from_slice(apu.as_bytes());
                buf.extend_from_slice(&to_32b_buf(apv.len() as u32));
                buf.extend_from_slice(apv.as_bytes());
                buf.extend_from_slice(&to_32b_buf(256));
                Ok(digest::digest(&digest::SHA256, &buf)?)
            },
        )?;

        // Part 2: decrypt the payload with the obtained secret
        assert_eq!(segments[1].len(), 0); // Encrypted Key is zero-length.
        let iv = base64::decode_config(&segments[2], base64::URL_SAFE_NO_PAD)?;
        let ciphertext = base64::decode_config(&segments[3], base64::URL_SAFE_NO_PAD)?;
        let auth_tag = base64::decode_config(&segments[4], base64::URL_SAFE_NO_PAD)?;
        assert_eq!(auth_tag.len(), 128 / 8);
        assert_eq!(iv.len(), 96 / 8);
        let opening_key = aead::OpeningKey::new(&aead::AES_256_GCM, &secret)
            .map_err(|_| ErrorKind::KeyImportFailed)?;
        let mut in_out = ciphertext.to_vec();
        in_out.append(&mut auth_tag.to_vec());
        // We have already asserted that iv is 12 bytes long.
        let nonce = aead::Nonce::try_assume_unique_for_key(&iv).expect("iv was not 12 bytes long.");
        let aad = aead::Aad::from(segments[0].as_bytes());
        let plaintext = aead::open_in_place(&opening_key, nonce, aad, 0, &mut in_out)
            .map_err(|_| ErrorKind::AEADOpenFailure)?;
        String::from_utf8(plaintext.to_vec()).map_err(Into::into)
    }
}

fn to_32b_buf(n: u32) -> Vec<u8> {
    let mut buf = [0; 4];
    BigEndian::write_u32(&mut buf, n);
    buf.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::test::rand::FixedSliceRandom;

    #[test]
    fn test_flow() {
        let fake_rng = FixedSliceRandom {
            bytes: &[
                81, 172, 131, 226, 73, 255, 225, 1, 239, 46, 242, 203, 73, 38, 128, 53, 240, 212,
                167, 208, 28, 66, 119, 80, 187, 244, 232, 133, 2, 168, 202, 127,
            ],
        };
        let flow = ScopedKeysFlow::with_random_key(&fake_rng).unwrap();
        let json = flow.generate_keys_jwk().unwrap();
        assert_eq!(json, "{\"crv\":\"P-256\",\"kty\":\"EC\",\"x\":\"ARvGIPJ5eIFdp6YTM-INVDqwfun2R9FfCUvXbH7QCIU\",\"y\":\"hk8gP0Po8nBh-WSiTsvsyesC5c1L6fGOEVuX8FHsvTs\"}");

        let jwe = "eyJhbGciOiJFQ0RILUVTIiwia2lkIjoiNFBKTTl5dGVGeUtsb21ILWd2UUtyWGZ0a0N3ak9HNHRfTmpYVXhLM1VqSSIsImVwayI6eyJrdHkiOiJFQyIsImNydiI6IlAtMjU2IiwieCI6IlB3eG9Na1RjSVZ2TFlKWU4wM2R0Y3o2TEJrR0FHaU1hZWlNQ3lTZXEzb2MiLCJ5IjoiLUYtTllRRDZwNUdSQ2ZoYm1hN3NvNkhxdExhVlNub012S0pFcjFBeWlaSSJ9LCJlbmMiOiJBMjU2R0NNIn0..b9FPhjjpmAmo_rP8.ur9jTry21Y2trvtcanSFmAtiRfF6s6qqyg6ruRal7PCwa7PxDzAuMN6DZW5BiK8UREOH08-FyRcIgdDOm5Zq8KwVAn56PGfcH30aNDGQNkA_mpfjx5Tj2z8kI6ryLWew4PGZb-PsL1g-_eyXhktq7dAhetjNYttKwSREWQFokv7N3nJGpukBqnwL1ost-MjDXlINZLVJKAiMHDcu-q7Epitwid2c2JVGOSCJjbZ4-zbxVmZ4o9xhFb2lbvdiaMygH6bPlrjEK99uT6XKtaIZmyDwftbD6G3x4On-CqA2TNL6ILRaJMtmyX--ctL0IrngUIHg_F0Wz94v.zBD8NACkUcZTPLH0tceGnA";
        let keys = flow.decrypt_keys_jwe(jwe).unwrap();
        assert_eq!(keys, "{\"https://identity.mozilla.com/apps/oldsync\":{\"kty\":\"oct\",\"scope\":\"https://identity.mozilla.com/apps/oldsync\",\"k\":\"8ek1VNk4sjrNP0DhGC4crzQtwmpoR64zHuFMHb4Tw-exR70Z2SSIfMSrJDTLEZid9lD05-hbA3n2Q4Esjlu1tA\",\"kid\":\"1526414944666-zgTjf5oXmPmBjxwXWFsDWg\"}}");
    }
}
