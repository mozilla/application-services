/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Implements Elliptic-Curve Diffie-Hellman for JWE - specifically, the
//! "Ephemeral-Static direct key agreement" mode described in
//! https://tools.ietf.org/html/rfc7518#section-4.6

use crate::{
    aes,
    error::{JwCryptoError, Result},
    Algorithm, CompactJwe, EncryptionAlgorithm, JweHeader, Jwk, JwkKeyParameters,
};
use rc_crypto::{
    agreement::{self, EphemeralKeyPair, InputKeyMaterial, UnparsedPublicKey},
    digest,
};
use serde_derive::{Deserialize, Serialize};

/// Key params specific to ECDH encryption.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ECKeysParameters {
    pub crv: String,
    pub x: String,
    pub y: String,
}

/// The ECDH helper that takes the cleartext and key, creates the appropriate
/// header, then calls the `aes` module to do the actual encryption.
pub(crate) fn encrypt_to_jwe(
    data: &[u8],
    enc: EncryptionAlgorithm,
    peer_jwk: &Jwk,
) -> Result<CompactJwe> {
    let local_key_pair = EphemeralKeyPair::generate(&agreement::ECDH_P256)?;
    let local_public_key = extract_pub_key_jwk(&local_key_pair)?;
    let ec_key_params = match peer_jwk.key_parameters {
        JwkKeyParameters::EC(ref params) => params,
        _ => return Err(JwCryptoError::IllegalState("Not an EC key")),
    };
    let protected_header = JweHeader {
        kid: peer_jwk.kid.clone(),
        alg: Algorithm::ECDH_ES,
        enc,
        epk: Some(local_public_key),
        apu: None,
        apv: None,
    };
    let secret = derive_shared_secret(&protected_header, local_key_pair, ec_key_params)?;
    match protected_header.enc {
        EncryptionAlgorithm::A256GCM => {
            aes::aes_gcm_encrypt(data, protected_header, secret.as_ref())
        }
    }
}

/// The ECDH helper that takes the ciphertext in the form of a CompactJwe,
/// and the keys, creates the appropriate header, then calls the `aes` module to
/// do the actual decrytion.
pub(crate) fn decrypt_jwe(jwe: &CompactJwe, local_key_pair: EphemeralKeyPair) -> Result<String> {
    // Part 0: Validate inputs.
    let protected_header = jwe.protected_header()?.ok_or(JwCryptoError::IllegalState(
        "protected_header must be present.",
    ))?;
    if protected_header.alg != Algorithm::ECDH_ES {
        return Err(JwCryptoError::IllegalState("alg mismatch."));
    }
    // `alg="ECDH-ES"` mandates no encrypted key.
    if jwe.encrypted_key()?.is_some() {
        return Err(JwCryptoError::IllegalState(
            "The Encrypted Key must be empty.",
        ));
    }

    // Part 1: Reconstruct the secret.
    let peer_jwk = protected_header
        .epk
        .as_ref()
        .ok_or(JwCryptoError::IllegalState("epk not present"))?;

    let ec_key_params = match peer_jwk.key_parameters {
        JwkKeyParameters::EC(ref params) => params,
        _ => return Err(JwCryptoError::IllegalState("Not an EC key")),
    };

    let secret = derive_shared_secret(&protected_header, local_key_pair, ec_key_params)?;

    // Part 2: decrypt the payload
    match protected_header.enc {
        EncryptionAlgorithm::A256GCM => aes::aes_gcm_decrypt(jwe, secret.as_ref()),
    }
}

fn derive_shared_secret(
    protected_header: &JweHeader,
    local_key_pair: EphemeralKeyPair,
    peer_key: &ECKeysParameters,
) -> Result<digest::Digest> {
    let (private_key, _) = local_key_pair.split();
    let peer_public_key_raw_bytes = public_key_from_ec_params(peer_key)?;
    let peer_public_key = UnparsedPublicKey::new(&agreement::ECDH_P256, &peer_public_key_raw_bytes);
    // Note: We don't support key-wrapping, but if we did `algorithm_id` would be `alg` instead.
    let algorithm_id = protected_header.enc.algorithm_id();
    let ikm = private_key.agree(&peer_public_key)?;
    let apu = protected_header.apu.as_deref().unwrap_or_default();
    let apv = protected_header.apv.as_deref().unwrap_or_default();
    get_secret_from_ikm(ikm, apu, apv, algorithm_id)
}

fn public_key_from_ec_params(jwk: &ECKeysParameters) -> Result<Vec<u8>> {
    let x = base64::decode_config(&jwk.x, base64::URL_SAFE_NO_PAD)?;
    let y = base64::decode_config(&jwk.y, base64::URL_SAFE_NO_PAD)?;
    if jwk.crv != "P-256" {
        return Err(JwCryptoError::PartialImplementation(
            "Only P-256 curves are supported.",
        ));
    }
    if x.len() != (256 / 8) {
        return Err(JwCryptoError::IllegalState("X must be 32 bytes long."));
    }
    if y.len() != (256 / 8) {
        return Err(JwCryptoError::IllegalState("Y must be 32 bytes long."));
    }
    let mut peer_pub_key: Vec<u8> = vec![0x04];
    peer_pub_key.extend_from_slice(&x);
    peer_pub_key.extend_from_slice(&y);
    Ok(peer_pub_key)
}

fn get_secret_from_ikm(
    ikm: InputKeyMaterial,
    apu: &str,
    apv: &str,
    alg: &str,
) -> Result<digest::Digest> {
    let secret = ikm.derive(|z| {
        let mut buf: Vec<u8> = vec![];
        // ConcatKDF (1 iteration since keyLen <= hashLen).
        // See rfc7518 section 4.6 for reference.
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(z);
        // otherinfo
        buf.extend_from_slice(&(alg.len() as u32).to_be_bytes());
        buf.extend_from_slice(alg.as_bytes());
        buf.extend_from_slice(&(apu.len() as u32).to_be_bytes());
        buf.extend_from_slice(apu.as_bytes());
        buf.extend_from_slice(&(apv.len() as u32).to_be_bytes());
        buf.extend_from_slice(apv.as_bytes());
        buf.extend_from_slice(&256u32.to_be_bytes());
        digest::digest(&digest::SHA256, &buf)
    })?;
    Ok(secret)
}

/// Extracts the public key from an [EphemeralKeyPair] as a [Jwk].
pub fn extract_pub_key_jwk(key_pair: &EphemeralKeyPair) -> Result<Jwk> {
    let pub_key_bytes = key_pair.public_key().to_bytes()?;
    // Uncompressed form (see SECG SEC1 section 2.3.3).
    // First byte is 4, then 32 bytes for x, and 32 bytes for y.
    assert_eq!(pub_key_bytes.len(), 1 + 32 + 32);
    assert_eq!(pub_key_bytes[0], 0x04);
    let x = Vec::from(&pub_key_bytes[1..33]);
    let x = base64::encode_config(&x, base64::URL_SAFE_NO_PAD);
    let y = Vec::from(&pub_key_bytes[33..]);
    let y = base64::encode_config(&y, base64::URL_SAFE_NO_PAD);
    Ok(Jwk {
        kid: None,
        key_parameters: JwkKeyParameters::EC(ECKeysParameters {
            crv: "P-256".to_owned(),
            x,
            y,
        }),
    })
}

#[test]
fn test_encrypt_decrypt_jwe_ecdh_es() {
    use super::{decrypt_jwe, encrypt_to_jwe, DecryptionParameters, EncryptionParameters};
    use rc_crypto::agreement;
    let key_pair = EphemeralKeyPair::generate(&agreement::ECDH_P256).unwrap();
    let jwk = extract_pub_key_jwk(&key_pair).unwrap();
    let data = b"The big brown fox jumped over... What?";
    let encrypted = encrypt_to_jwe(
        data,
        EncryptionParameters::ECDH_ES {
            enc: EncryptionAlgorithm::A256GCM,
            peer_jwk: &jwk,
        },
    )
    .unwrap();
    let decrypted = decrypt_jwe(
        &encrypted,
        DecryptionParameters::ECDH_ES {
            local_key_pair: key_pair,
        },
    )
    .unwrap();
    assert_eq!(decrypted, std::str::from_utf8(data).unwrap());
}

#[test]
fn test_bad_key_type() {
    use super::{encrypt_to_jwe, EncryptionParameters};
    use crate::error::JwCryptoError;
    let key_pair = EphemeralKeyPair::generate(&agreement::ECDH_P256).unwrap();
    let jwk = extract_pub_key_jwk(&key_pair).unwrap();
    let data = b"The big brown fox fell down";
    assert!(matches!(
        encrypt_to_jwe(
            data,
            EncryptionParameters::Direct {
                enc: EncryptionAlgorithm::A256GCM,
                jwk: &jwk
            },
        ),
        Err(JwCryptoError::IllegalState(_))
    ));
}
