/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Support for "Direct Encryption with a Shared Symmetric Key"
//! See https://tools.ietf.org/html/rfc7518#section-4.5 for all the details.

use crate::{
    aes,
    error::{JwCryptoError, Result},
    Algorithm, CompactJwe, EncryptionAlgorithm, JweHeader, Jwk, JwkKeyParameters,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rc_crypto::rand;

impl Jwk {
    /// Create a new random key suitable for `Direct` symmetric encryption.
    /// Consumers can store this for later use, probably via serde serialization.
    pub fn new_direct_key(kid: Option<String>) -> Result<Self> {
        // We only support AES256 which has a 32byte key.
        let mut bytes: Vec<u8> = vec![0; 32];
        rand::fill(&mut bytes)?;
        Ok(Jwk {
            kid,
            key_parameters: JwkKeyParameters::Direct {
                k: URL_SAFE_NO_PAD.encode(&bytes),
            },
        })
    }

    // Create a new Jwk given the raw bytes of a `Direct` key. We generally
    // prefer consumers to use serde with the entire key.
    pub fn new_direct_from_bytes(kid: Option<String>, key: &[u8]) -> Self {
        Jwk {
            kid,
            key_parameters: JwkKeyParameters::Direct {
                k: URL_SAFE_NO_PAD.encode(key),
            },
        }
    }
}

pub(crate) fn encrypt_to_jwe(
    data: &[u8],
    enc: EncryptionAlgorithm,
    jwk: &Jwk,
) -> Result<CompactJwe> {
    // It's slightly unfortunate we need to supply a struct with ECDH specific
    // values all specified as None, but doesn't seem likely to ever actually hurt.
    let protected_header = JweHeader {
        kid: jwk.kid.clone(),
        alg: Algorithm::Direct,
        enc,
        epk: None,
        apu: None,
        apv: None,
    };
    let secret = match &jwk.key_parameters {
        JwkKeyParameters::Direct { k } => URL_SAFE_NO_PAD.decode(k)?,
        _ => return Err(JwCryptoError::IllegalState("Not a Direct key")),
    };
    aes::aes_gcm_encrypt(data, protected_header, &secret)
}

pub(crate) fn decrypt_jwe(jwe: &CompactJwe, jwk: Jwk) -> Result<String> {
    let secret = match jwk.key_parameters {
        JwkKeyParameters::Direct { k } => URL_SAFE_NO_PAD.decode(k)?,
        _ => return Err(JwCryptoError::IllegalState("Not a Direct key")),
    };
    // `alg="dir"` mandates no encrypted key.
    if jwe.encrypted_key()?.is_some() {
        return Err(JwCryptoError::IllegalState(
            "The Encrypted Key must be empty.",
        ));
    }
    aes::aes_gcm_decrypt(jwe, &secret)
}

#[test]
fn test_simple_roundtrip() {
    // We should be able to round-trip data.
    use super::{decrypt_jwe, encrypt_to_jwe, DecryptionParameters, EncryptionParameters};
    use nss::ensure_initialized;

    ensure_initialized();

    let jwk = Jwk::new_direct_key(Some("my key".to_string())).unwrap();
    let data = "to be, or not 🐝🐝";
    let encrypted = encrypt_to_jwe(
        data.as_bytes(),
        EncryptionParameters::Direct {
            jwk: &jwk,
            enc: EncryptionAlgorithm::A256GCM,
        },
    )
    .unwrap();
    let decrypted = decrypt_jwe(&encrypted, DecryptionParameters::Direct { jwk }).unwrap();
    assert_eq!(data, decrypted);
}

#[test]
fn test_modified_ciphertext() {
    // Modifying the ciphertext will fail.
    use super::{decrypt_jwe, encrypt_to_jwe, DecryptionParameters, EncryptionParameters};
    use nss::ensure_initialized;
    use std::str::FromStr;

    ensure_initialized();

    let jwk = Jwk::new_direct_key(Some("my key".to_string())).unwrap();
    let data = "to be, or not 🐝🐝";
    let encrypted = encrypt_to_jwe(
        data.as_bytes(),
        EncryptionParameters::Direct {
            jwk: &jwk,
            enc: EncryptionAlgorithm::A256GCM,
        },
    )
    .unwrap();
    // additional text
    assert!(matches!(
        decrypt_jwe(
            &(encrypted.clone() + "A"),
            DecryptionParameters::Direct { jwk: jwk.clone() }
        ),
        Err(JwCryptoError::IllegalState(_))
    ));
    // truncated text
    assert!(matches!(
        decrypt_jwe(
            &(encrypted[0..encrypted.len() - 2]),
            DecryptionParameters::Direct { jwk: jwk.clone() }
        ),
        Err(JwCryptoError::IllegalState(_))
    ));
    // modified ciphertext - to make this test meaningful we need to
    // reconsitute the CompactJwe and modify that, otherwise we are just going
    // to get a base64 or json error.
    let jwe = CompactJwe::from_str(&encrypted).unwrap();
    let mut new_ciphertext = jwe.ciphertext().unwrap();
    new_ciphertext[0] = new_ciphertext[0].wrapping_add(1);
    let jwe_modified = CompactJwe::new(
        jwe.protected_header().unwrap(),
        jwe.encrypted_key().unwrap(),
        jwe.iv().unwrap(),
        new_ciphertext,
        jwe.auth_tag().unwrap(),
    )
    .unwrap();

    // phew - finally (fail to) decrypt the modified ciphertext.
    assert!(matches!(
        decrypt_jwe(
            &jwe_modified.to_string(),
            DecryptionParameters::Direct { jwk }
        ),
        Err(JwCryptoError::CryptoError(_))
    ));
}

#[test]
fn test_iv() {
    // Encrypting the same thing twice should give different payloads due to
    // different IV.
    use super::{encrypt_to_jwe, EncryptionParameters};
    use nss::ensure_initialized;

    ensure_initialized();

    let jwk = Jwk::new_direct_key(Some("my key".to_string())).unwrap();
    let data = "to be, or not 🐝🐝";
    let e1 = encrypt_to_jwe(
        data.as_bytes(),
        EncryptionParameters::Direct {
            enc: EncryptionAlgorithm::A256GCM,
            jwk: &jwk,
        },
    )
    .unwrap();
    let e2 = encrypt_to_jwe(
        data.as_bytes(),
        EncryptionParameters::Direct {
            jwk: &jwk,
            enc: EncryptionAlgorithm::A256GCM,
        },
    )
    .unwrap();
    assert_ne!(e1, e2);
}

#[test]
fn test_jose() {
    // ciphertext generated by node-jose via:
    /*
    const parseJwk = require("jose/jwk/parse").default;
    const CompactEncrypt = require("jose/jwe/compact/encrypt").default;
    const encoder = new TextEncoder();
    const payload = "Hello, World!";
    const key = "asecret256bitkeyasecret256bitkey";
    parseJwk({kty: "oct", k: Buffer.from(key).toString("base64")}, "A256GCM").then(key => {
        new CompactEncrypt(encoder.encode(payload))
            .setProtectedHeader({ alg: "dir", enc: "A256GCM" })
            .encrypt(key)
            .then(jwe => {
                console.log(jwe);
            });
    })
    */
    // (A note for future readers - we tried using python-jose, but it
    // generated a 16 byte nonce, where the spec clearly calls for exactly 12
    // bytes. We could decrypt that python-jose payload if we modified
    // `Nonce::try_assume_unique_for_key()` to allow a longer key, but we don't
    // want to do that until we have evidence it's actually spec compliant.)
    use super::{decrypt_jwe, DecryptionParameters};
    use nss::ensure_initialized;

    ensure_initialized();

    let jwk = Jwk::new_direct_from_bytes(None, "asecret256bitkeyasecret256bitkey".as_bytes());
    let ciphertext = "eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..nhKdQEKqoKPzfCda.rQOj0Nfs6wO5Gj4Quw.CMJFS9YBADLLePdj1sssSg";
    let decrypted = decrypt_jwe(ciphertext, DecryptionParameters::Direct { jwk }).unwrap();
    assert_eq!(decrypted, "Hello, World!");
}

#[test]
fn test_bad_key() {
    use super::{decrypt_jwe, DecryptionParameters};
    use crate::error::JwCryptoError;
    use nss::ensure_initialized;

    ensure_initialized();

    let jwk = Jwk::new_direct_from_bytes(None, "a_wrong256bitkeya_wrong256bitkey".as_bytes());
    let ciphertext = "eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..nhKdQEKqoKPzfCda.rQOj0Nfs6wO5Gj4Quw.CMJFS9YBADLLePdj1sssSg";
    assert!(matches!(
        decrypt_jwe(ciphertext, DecryptionParameters::Direct { jwk }),
        Err(JwCryptoError::CryptoError(_))
    ));
}

#[test]
fn test_bad_key_type() {
    use super::{encrypt_to_jwe, EncryptionParameters};
    use crate::error::JwCryptoError;
    use nss::ensure_initialized;

    ensure_initialized();

    let jwk = Jwk::new_direct_key(Some("my key".to_string())).unwrap();
    let data = b"The big brown fox fell down";
    assert!(matches!(
        encrypt_to_jwe(
            data,
            EncryptionParameters::ECDH_ES {
                enc: EncryptionAlgorithm::A256GCM,
                peer_jwk: &jwk,
            },
        ),
        Err(JwCryptoError::IllegalState(_))
    ));
}

#[test]
fn test_bad_key_type_direct() {
    use super::{EncryptionAlgorithm, EphemeralKeyPair};
    use nss::ensure_initialized;
    use rc_crypto::agreement;

    use crate::error::JwCryptoError;

    ensure_initialized();

    let key_pair = EphemeralKeyPair::generate(&agreement::ECDH_P256).unwrap();
    let jwk = crate::ec::extract_pub_key_jwk(&key_pair).unwrap();

    let data = b"The big brown fox fell down";
    assert!(matches!(
        encrypt_to_jwe(data, EncryptionAlgorithm::A256GCM, &jwk,),
        Err(JwCryptoError::IllegalState(_))
    ));
}
