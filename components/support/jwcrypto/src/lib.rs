/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A library for using JSON Object Signing and Encryption (JOSE) data formats
//! such as JWE and JWK, as described in https://tools.ietf.org/html/rfc7518
//! and related standards.
//! The encryption is done by [rc_crypto] - this crate just does the JOSE
//! wrappers around this crypto. As a result, most of the structs etc here
//! support serialization and deserialization to and from JSON via serde in
//! a way that's compatibile with rfc7518 etc.

// Theoretically, everything done in this crate could and should be done in a JWT library.
// However, none of the existing rust JWT libraries can handle ECDH-ES encryption, and API choices
// made by their authors make it difficult to add this feature.
// In the past, we chose cjose to do that job, but it added three C dependencies to build and link
// against: jansson, openssl and cjose itself.
// So now, this *is* our JWT library.

pub use error::JwCryptoError;
use error::Result;
use rc_crypto::agreement::EphemeralKeyPair;
use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;

mod aes;
mod direct;
pub mod ec;
mod error;

/// Specifies the mode, algorithm and keys of the encryption operation.
pub enum EncryptionParameters<'a> {
    // ECDH-ES in Direct Key Agreement mode.
    #[allow(non_camel_case_types)]
    ECDH_ES {
        enc: EncryptionAlgorithm,
        peer_jwk: &'a Jwk,
    },
    // Direct Encryption with a shared symmetric key.
    Direct {
        enc: EncryptionAlgorithm,
        jwk: &'a Jwk,
    },
}

/// Specifies the mode and keys of the decryption operation.
pub enum DecryptionParameters {
    // ECDH-ES in Direct Key Agreement mode.
    #[allow(non_camel_case_types)]
    ECDH_ES {
        local_key_pair: EphemeralKeyPair,
    },
    // Direct with a shared symmetric key.
    Direct {
        jwk: Jwk,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
enum Algorithm {
    #[serde(rename = "ECDH-ES")]
    #[allow(non_camel_case_types)]
    ECDH_ES,
    #[serde(rename = "dir")]
    Direct,
}

/// The encryption algorithms supported by this crate.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    A256GCM,
}

impl EncryptionAlgorithm {
    fn algorithm_id(&self) -> &'static str {
        match self {
            Self::A256GCM => "A256GCM",
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct JweHeader {
    alg: Algorithm,
    enc: EncryptionAlgorithm,
    #[serde(skip_serializing_if = "Option::is_none")]
    kid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    epk: Option<Jwk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    apu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    apv: Option<String>,
}

/// Defines the key to use for all operations in this crate.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Jwk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
    #[serde(flatten)]
    pub key_parameters: JwkKeyParameters,
}

/// The enum passed in to hold the encryption and decryption keys. The variant
/// of the enum must match the variant of the Encryption/Decryption parameters
/// or the encryption/decryption operations will fail.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kty")]
pub enum JwkKeyParameters {
    /// When doing ECDH (asymmetric) encryption, you specify elliptic curve points.
    EC(ec::ECKeysParameters),
    /// When doing Direct (symmetric) encryption, you specify random bytes of
    /// the appropriate length, base64 encoded.
    #[serde(rename = "oct")] // rfc7518 section-6.1 specifies "oct" as key-type...
    Direct { k: String }, // ...and "k" for the base64 value.
}

/// Internal representation of a CompactJwe. The public interface of this
/// crate is all via strings, so it's not public.
#[derive(Debug)]
struct CompactJwe {
    jwe_segments: Vec<String>,
}

impl CompactJwe {
    // A builder pattern would be nicer, but this will do for now.
    fn new(
        protected_header: Option<JweHeader>,
        encrypted_key: Option<Vec<u8>>,
        iv: Option<Vec<u8>>,
        ciphertext: Vec<u8>,
        auth_tag: Option<Vec<u8>>,
    ) -> Result<Self> {
        let protected_header = protected_header
            .as_ref()
            .map(|h| serde_json::to_string(&h))
            .transpose()?
            .map(|h| base64::encode_config(&h, base64::URL_SAFE_NO_PAD))
            .unwrap_or_default();
        let encrypted_key = encrypted_key
            .as_ref()
            .map(|k| base64::encode_config(&k, base64::URL_SAFE_NO_PAD))
            .unwrap_or_default();
        let iv = iv
            .as_ref()
            .map(|iv| base64::encode_config(&iv, base64::URL_SAFE_NO_PAD))
            .unwrap_or_default();
        let ciphertext = base64::encode_config(&ciphertext, base64::URL_SAFE_NO_PAD);
        let auth_tag = auth_tag
            .as_ref()
            .map(|t| base64::encode_config(&t, base64::URL_SAFE_NO_PAD))
            .unwrap_or_default();
        let jwe_segments = vec![protected_header, encrypted_key, iv, ciphertext, auth_tag];
        Ok(Self { jwe_segments })
    }

    fn protected_header(&self) -> Result<Option<JweHeader>> {
        Ok(self
            .try_deserialize_base64_segment(0)?
            .map(|s| serde_json::from_slice(&s))
            .transpose()?)
    }

    fn protected_header_raw(&self) -> &str {
        &self.jwe_segments[0]
    }

    fn encrypted_key(&self) -> Result<Option<Vec<u8>>> {
        self.try_deserialize_base64_segment(1)
    }

    fn iv(&self) -> Result<Option<Vec<u8>>> {
        self.try_deserialize_base64_segment(2)
    }

    fn ciphertext(&self) -> Result<Vec<u8>> {
        self.try_deserialize_base64_segment(3)?
            .ok_or(JwCryptoError::IllegalState("Ciphertext is empty"))
    }

    fn auth_tag(&self) -> Result<Option<Vec<u8>>> {
        self.try_deserialize_base64_segment(4)
    }

    fn try_deserialize_base64_segment(&self, index: usize) -> Result<Option<Vec<u8>>> {
        Ok(match self.jwe_segments[index].is_empty() {
            true => None,
            false => Some(base64::decode_config(
                &self.jwe_segments[index],
                base64::URL_SAFE_NO_PAD,
            )?),
        })
    }
}

impl FromStr for CompactJwe {
    type Err = JwCryptoError;
    fn from_str(str: &str) -> Result<Self> {
        let jwe_segments: Vec<String> = str.split('.').map(|s| s.to_owned()).collect();
        if jwe_segments.len() != 5 {
            return Err(JwCryptoError::DeserializationError);
        }
        Ok(Self { jwe_segments })
    }
}

impl ToString for CompactJwe {
    fn to_string(&self) -> String {
        assert!(self.jwe_segments.len() == 5);
        self.jwe_segments.join(".")
    }
}

/// Encrypt and serialize data in the JWE compact form.
pub fn encrypt_to_jwe(data: &[u8], encryption_params: EncryptionParameters) -> Result<String> {
    let jwe = match encryption_params {
        EncryptionParameters::ECDH_ES { enc, peer_jwk } => ec::encrypt_to_jwe(data, enc, peer_jwk),
        EncryptionParameters::Direct { enc, jwk } => direct::encrypt_to_jwe(data, enc, jwk),
    }?;
    Ok(jwe.to_string())
}

/// Deserialize and decrypt data in the JWE compact form.
pub fn decrypt_jwe(jwe: &str, decryption_params: DecryptionParameters) -> Result<String> {
    let jwe = jwe.parse()?;
    match decryption_params {
        DecryptionParameters::ECDH_ES { local_key_pair } => ec::decrypt_jwe(&jwe, local_key_pair),
        DecryptionParameters::Direct { jwk } => direct::decrypt_jwe(&jwe, jwk),
    }
}

#[test]
fn test_jwk_ec_deser_with_kid() {
    let jwk = Jwk {
        kid: Some("the-key-id".to_string()),
        key_parameters: JwkKeyParameters::EC(ec::ECKeysParameters {
            crv: "CRV".to_string(),
            x: "X".to_string(),
            y: "Y".to_string(),
        }),
    };
    let jstr = serde_json::to_string(&jwk).unwrap();
    // Make sure all the tags get the right info by checking the literal string.
    assert_eq!(
        jstr,
        r#"{"kid":"the-key-id","kty":"EC","crv":"CRV","x":"X","y":"Y"}"#
    );
    // And check it round-trips.
    assert_eq!(jwk, serde_json::from_str(&jstr).unwrap());
}

#[test]
fn test_jwk_deser_no_kid() {
    let jwk = Jwk {
        kid: None,
        key_parameters: JwkKeyParameters::EC(ec::ECKeysParameters {
            crv: "CRV".to_string(),
            x: "X".to_string(),
            y: "Y".to_string(),
        }),
    };
    let jstr = serde_json::to_string(&jwk).unwrap();
    // Make sure all the tags get the right info by checking the literal string.
    assert_eq!(jstr, r#"{"kty":"EC","crv":"CRV","x":"X","y":"Y"}"#);
    // And check it round-trips.
    assert_eq!(jwk, serde_json::from_str(&jstr).unwrap());
}

#[test]
fn test_jwk_direct_deser_with_kid() {
    let jwk = Jwk::new_direct_from_bytes(Some("key-id".to_string()), &[0, 1, 2, 3]);
    let jstr = serde_json::to_string(&jwk).unwrap();
    // Make sure all the tags get the right info by checking the literal string.
    assert_eq!(jstr, r#"{"kid":"key-id","kty":"oct","k":"AAECAw"}"#);
    // And check it round-trips.
    assert_eq!(jwk, serde_json::from_str(&jstr).unwrap());
}

#[test]
fn test_compact_jwe_roundtrip() {
    let mut iv = [0u8; 16];
    rc_crypto::rand::fill(&mut iv).unwrap();
    let mut ciphertext = [0u8; 243];
    rc_crypto::rand::fill(&mut ciphertext).unwrap();
    let mut auth_tag = [0u8; 16];
    rc_crypto::rand::fill(&mut auth_tag).unwrap();
    let jwe = CompactJwe::new(
        Some(JweHeader {
            alg: Algorithm::ECDH_ES,
            enc: EncryptionAlgorithm::A256GCM,
            kid: None,
            epk: None,
            apu: None,
            apv: None,
        }),
        None,
        Some(iv.to_vec()),
        ciphertext.to_vec(),
        Some(auth_tag.to_vec()),
    )
    .unwrap();
    let compacted = jwe.to_string();
    let jwe2: CompactJwe = compacted.parse().unwrap();
    assert_eq!(jwe.jwe_segments, jwe2.jwe_segments);
}
