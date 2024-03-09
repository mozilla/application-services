/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Module providing all the cryptography needed by the push component
//!
//! Mainly exports a trait [`Cryptography`] and a concrete type that implements that trait
//! [`Crypto`]
//!
//! The push component encrypts its push notifications. When a subscription is created,
//! [`Cryptography::generate_key`] is called to generate a public/private key pair.
//!
//! The public key is then given to the subscriber (for example, Firefox Accounts) and the private key
//! is persisted in the client. Subscribers are required to encrypt their payloads using the public key and
//! when delivered to the client, the client would load the private key from storage and decrypt the payload.
//!

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::str::FromStr;

use crate::{error, PushError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rc_crypto::ece::{self, EcKeyComponents, LocalKeyPair};
use rc_crypto::ece_crypto::RcCryptoLocalKeyPair;
use rc_crypto::rand;
use serde::{Deserialize, Serialize};

pub const SER_AUTH_LENGTH: usize = 16;
pub type Decrypted = Vec<u8>;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) enum VersionnedKey<'a> {
    V1(Cow<'a, KeyV1>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CryptoEncoding {
    Aesgcm,
    Aes128gcm,
}

impl FromStr for CryptoEncoding {
    type Err = PushError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "aesgcm" => Self::Aesgcm,
            "aes128gcm" => Self::Aes128gcm,
            _ => {
                return Err(PushError::CryptoError(format!(
                    "Invalid crypto encoding {}",
                    s
                )))
            }
        })
    }
}

impl Display for CryptoEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Aesgcm => "aesgcm",
                Self::Aes128gcm => "aes128gcm",
            }
        )
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyV1 {
    pub(crate) p256key: EcKeyComponents,
    pub(crate) auth: Vec<u8>,
}
pub type Key = KeyV1;

impl std::fmt::Debug for KeyV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyV1").finish()
    }
}

impl Key {
    // We define this method so the type-checker prevents us from
    // trying to serialize `Key` directly since `bincode::serialize`
    // would compile because both types derive `Serialize`.
    pub(crate) fn serialize(&self) -> error::Result<Vec<u8>> {
        Ok(bincode::serialize(&VersionnedKey::V1(Cow::Borrowed(self)))?)
    }

    pub(crate) fn deserialize(bytes: &[u8]) -> error::Result<Self> {
        let versionned = bincode::deserialize(bytes)?;
        match versionned {
            VersionnedKey::V1(prv_key) => Ok(prv_key.into_owned()),
        }
    }

    pub fn key_pair(&self) -> &EcKeyComponents {
        &self.p256key
    }

    pub fn auth_secret(&self) -> &[u8] {
        &self.auth
    }

    pub fn private_key(&self) -> &[u8] {
        self.p256key.private_key()
    }

    pub fn public_key(&self) -> &[u8] {
        self.p256key.public_key()
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait Cryptography: Default {
    /// generate a new local EC p256 key
    fn generate_key() -> error::Result<Key>;

    /// General decrypt function. Calls to decrypt_aesgcm or decrypt_aes128gcm as needed.
    #[allow(clippy::needless_lifetimes)]
    // Clippy complains here although the lifetime is needed, seems like a bug with automock
    fn decrypt<'a>(key: &Key, push_payload: PushPayload<'a>) -> error::Result<Decrypted>;

    /// Decrypt the obsolete "aesgcm" format (which is still used by a number of providers)
    fn decrypt_aesgcm(
        key: &Key,
        content: &[u8],
        salt: Option<Vec<u8>>,
        crypto_key: Option<Vec<u8>>,
    ) -> error::Result<Decrypted>;

    /// Decrypt the RFC 8188 format.
    fn decrypt_aes128gcm(key: &Key, content: &[u8]) -> error::Result<Decrypted>;
}

#[derive(Default)]
pub struct Crypto;

pub fn get_random_bytes(size: usize) -> error::Result<Vec<u8>> {
    let mut bytes = vec![0u8; size];
    rand::fill(&mut bytes).map_err(|e| {
        error::PushError::CryptoError(format!("Could not generate random bytes: {:?}", e))
    })?;
    Ok(bytes)
}

/// Extract the sub-value from the header.
/// Sub values have the form of `label=value`. Due to a bug in some push providers, treat ',' and ';' as
/// equivalent.
fn extract_value(val: &str, target: &str) -> Option<Vec<u8>> {
    if !val.contains(&format!("{}=", target)) {
        error::debug!("No sub-value found for {}", target);
        return None;
    }
    let items = val.split([',', ';']);
    for item in items {
        let mut kv = item.split('=');
        if kv.next() == Some(target) {
            if let Some(val) = kv.next() {
                return match URL_SAFE_NO_PAD.decode(val) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        error_support::report_error!(
                            "push-base64-decode",
                            "base64 failed for target:{}; {:?}",
                            target,
                            e
                        );
                        None
                    }
                };
            }
        }
    }
    None
}

impl Cryptography for Crypto {
    fn generate_key() -> error::Result<Key> {
        rc_crypto::ensure_initialized();

        let key = RcCryptoLocalKeyPair::generate_random()?;
        let components = key.raw_components()?;
        let auth = get_random_bytes(SER_AUTH_LENGTH)?;
        Ok(Key {
            p256key: components,
            auth,
        })
    }

    fn decrypt(key: &Key, push_payload: PushPayload<'_>) -> error::Result<Decrypted> {
        rc_crypto::ensure_initialized();
        // convert the private key into something useful.
        let d_salt = extract_value(push_payload.salt, "salt");
        let d_dh = extract_value(push_payload.dh, "dh");
        let d_body = URL_SAFE_NO_PAD.decode(push_payload.body)?;

        match CryptoEncoding::from_str(push_payload.encoding)? {
            CryptoEncoding::Aesgcm => Self::decrypt_aesgcm(key, &d_body, d_salt, d_dh),
            CryptoEncoding::Aes128gcm => Self::decrypt_aes128gcm(key, &d_body),
        }
    }

    fn decrypt_aesgcm(
        key: &Key,
        content: &[u8],
        salt: Option<Vec<u8>>,
        crypto_key: Option<Vec<u8>>,
    ) -> error::Result<Decrypted> {
        let dh = crypto_key
            .ok_or_else(|| error::PushError::CryptoError("Missing public key".to_string()))?;
        let salt = salt.ok_or_else(|| error::PushError::CryptoError("Missing salt".to_string()))?;
        let block = ece::legacy::AesGcmEncryptedBlock::new(&dh, &salt, 4096, content.to_vec())?;
        Ok(ece::legacy::decrypt_aesgcm(
            key.key_pair(),
            key.auth_secret(),
            &block,
        )?)
    }

    fn decrypt_aes128gcm(key: &Key, content: &[u8]) -> error::Result<Vec<u8>> {
        Ok(ece::decrypt(key.key_pair(), key.auth_secret(), content)?)
    }
}

#[derive(Debug, Deserialize)]
pub struct PushPayload<'a> {
    pub(crate) channel_id: &'a str,
    pub(crate) body: &'a str,
    pub(crate) encoding: &'a str,
    pub(crate) salt: &'a str,
    pub(crate) dh: &'a str,
}

impl<'a> TryFrom<&'a HashMap<String, String>> for PushPayload<'a> {
    type Error = PushError;

    fn try_from(value: &'a HashMap<String, String>) -> Result<Self, Self::Error> {
        let channel_id = value
            .get("chid")
            .ok_or_else(|| PushError::CryptoError("Invalid Push payload".to_string()))?;
        let body = value
            .get("body")
            .ok_or_else(|| PushError::CryptoError("Invalid Push payload".to_string()))?;
        let encoding = value.get("con").map(|s| s.as_str()).unwrap_or("aes128gcm");
        let salt = value.get("enc").map(|s| s.as_str()).unwrap_or("");
        let dh = value.get("cryptokey").map(|s| s.as_str()).unwrap_or("");
        Ok(Self {
            channel_id,
            body,
            encoding,
            salt,
            dh,
        })
    }
}

#[cfg(test)]
mod crypto_tests {
    use super::*;
    use nss::ensure_initialized;

    // generate unit test key
    fn test_key(priv_key: &str, pub_key: &str, auth: &str) -> Key {
        let components = EcKeyComponents::new(
            URL_SAFE_NO_PAD.decode(priv_key).unwrap(),
            URL_SAFE_NO_PAD.decode(pub_key).unwrap(),
        );
        let auth = URL_SAFE_NO_PAD.decode(auth).unwrap();
        Key {
            p256key: components,
            auth,
        }
    }

    const PLAINTEXT:&str = "Amidst the mists and coldest frosts I thrust my fists against the\nposts and still demand to see the ghosts.\n\n";

    fn decrypter(ciphertext: &str, encoding: &str, salt: &str, dh: &str) -> error::Result<Vec<u8>> {
        let priv_key_d = "qJkxxWGVVxy7BKvraNY3hg8Gs-Y8qi0lRaXWJ3R3aJ8";
        // The auth token
        let auth_raw = "LsuUOBKVQRY6-l7_Ajo-Ag";
        // This would be the public key sent to the subscription service.
        let pub_key_raw = "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50";

        let key = test_key(priv_key_d, pub_key_raw, auth_raw);
        Crypto::decrypt(
            &key,
            PushPayload {
                channel_id: "channel_id",
                body: ciphertext,
                encoding,
                salt,
                dh,
            },
        )
    }

    #[test]
    fn test_decrypt_aesgcm() {
        ensure_initialized();

        // The following comes from the delivered message body
        let ciphertext = "BNKu5uTFhjyS-06eECU9-6O61int3Rr7ARbm-xPhFuyDO5sfxVs-HywGaVonvzkarvfvXE9IRT_YNA81Og2uSqDasdMuw\
                          qm1zd0O3f7049IkQep3RJ2pEZTy5DqvI7kwMLDLzea9nroq3EMH5hYhvQtQgtKXeWieEL_3yVDQVg";
        // and now from the header values
        let dh = "keyid=foo;dh=BMOebOMWSRisAhWpRK9ZPszJC8BL9MiWvLZBoBU6pG6Kh6vUFSW4BHFMh0b83xCg3_7IgfQZXwmVuyu27vwiv5c,otherval=abcde";
        let salt = "salt=tSf2qu43C9BD0zkvRW5eUg";

        // and this is what it should be.

        let decrypted = decrypter(ciphertext, "aesgcm", salt, dh).unwrap();

        assert_eq!(String::from_utf8(decrypted).unwrap(), PLAINTEXT.to_string());
    }

    #[test]
    fn test_fail_decrypt_aesgcm() {
        ensure_initialized();

        let ciphertext = "BNKu5uTFhjyS-06eECU9-6O61int3Rr7ARbm-xPhFuyDO5sfxVs-HywGaVonvzkarvfvXE9IRT_\
                          YNA81Og2uSqDasdMuwqm1zd0O3f7049IkQep3RJ2pEZTy5DqvI7kwMLDLzea9nroq3EMH5hYhvQtQgtKXeWieEL_3yVDQVg";
        let dh = "dh=BMOebOMWSRisAhWpRK9ZPszJC8BL9MiWvLZBoBU6pG6Kh6vUFSW4BHFMh0b83xCg3_7IgfQZXwmVuyu27vwiv5c";
        let salt = "salt=SomeInvalidSaltValue";

        decrypter(ciphertext, "aesgcm", salt, dh).expect_err("Failed to abort, bad salt");
    }

    #[test]
    fn test_decrypt_aes128gcm() {
        ensure_initialized();

        let ciphertext = "Ek7iQgliMqS9kjFoiVOqRgAAEABBBFirfBtF6XTeHVPABFDveb1iu7uO1XVA_MYJeAo-\
             4ih8WYUsXSTIYmkKMv5_UB3tZuQI7BQ2EVpYYQfvOCrWZVMRL8fJCuB5wVXcoRoTaFJw\
             TlJ5hnw6IMSiaMqGVlc8drX7Hzy-ugzzAKRhGPV2x-gdsp58DZh9Ww5vHpHyT1xwVkXz\
             x3KTyeBZu4gl_zR0Q00li17g0xGsE6Dg3xlkKEmaalgyUyObl6_a8RA6Ko1Rc6RhAy2jdyY1LQbBUnA";

        let decrypted = decrypter(ciphertext, "aes128gcm", "", "").unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), PLAINTEXT.to_string());
    }
}
