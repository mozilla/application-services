/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles cryptographic functions.
//!
//! Depending on platform, this may call various libraries or have other dependencies.
//!
//! This uses prime256v1 EC encryption that should come from internal crypto calls. The "application-services"
//! module compiles openssl, however, so might be enough to tie into that.

use log;
use std::clone;
use std::cmp;
use std::fmt;

use ece::{
    Aes128GcmEceWebPushImpl, AesGcmEceWebPushImpl, AesGcmEncryptedBlock, LocalKeyPair,
    LocalKeyPairImpl,
};
use openssl::ec::EcKey;
use openssl::rand::rand_bytes;

use crate::error;

pub const SER_AUTH_LENGTH: usize = 16;
pub type Decrypted = Vec<u8>;

/// build the key off of the OpenSSL key implementation.
/// Much of this is taken from rust_ece/crypto/openssl/lib.rs
pub struct Key {
    /// A "Key" contains the cryptographic Web Push Key data.
    private: LocalKeyPairImpl,
    pub public: Vec<u8>,
    pub auth: Vec<u8>,
}

impl clone::Clone for Key {
    fn clone(&self) -> Key {
        Key {
            private: LocalKeyPairImpl::new(&self.private.to_raw()).unwrap(),
            public: self.public.clone(),
            auth: self.auth.clone(),
        }
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{private: {:?}, public: {:?}, auth: {:?}}}",
            base64::encode_config(&self.private.to_raw(), base64::URL_SAFE_NO_PAD),
            base64::encode_config(&self.public, base64::URL_SAFE_NO_PAD),
            base64::encode_config(&self.auth, base64::URL_SAFE_NO_PAD)
        )
    }
}

impl cmp::PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        self.private.to_raw() == other.private.to_raw()
            && self.public == other.public
            && self.auth == other.auth
    }
}

impl Key {
    /*
    re-instantiating the private key from a vector looks to be overly complex.
    */

    //TODO: Make these real serde functions
    /// Serialize a Key's private and auth information into a recoverable byte array.
    pub fn serialize(&self) -> error::Result<Vec<u8>> {
        // Unfortunately, EcKey::private_key_from_der(original.private_key_to_der())
        // produces a Key, but reading the public_key().to_bytes() fails with an
        // openssl "incompatible objects" error.
        // This does not bode well for doing actual functions with it.
        // So for now, hand serializing the Key.
        let mut result: Vec<u8> = Vec::new();
        let mut keypv = self.private.to_raw();
        let pvlen = keypv.len();
        // specify the version
        result.push(1);
        result.push(self.auth.len() as u8);
        result.append(&mut self.auth.clone());
        result.push(pvlen as u8);
        result.append(&mut keypv);
        Ok(result)
    }

    /// Recover a byte array into a Key structure.
    pub fn deserialize(raw: Vec<u8>) -> error::Result<Key> {
        if raw[0] != 1 {
            return Err(error::ErrorKind::EncryptionError(
                "Unknown Key Serialization version".to_owned(),
            )
            .into());
        }
        let mut start = 1;
        // TODO: Make the following a macro call.
        // fetch out the auth
        let mut l = raw[start] as usize;
        start += 1;
        let mut end = start + l;
        let auth = &raw[start..end];
        // get the private key
        l = raw[end] as usize;
        start = end + 1;
        end = start + l;
        // generate the private key from the components
        let private = match LocalKeyPairImpl::new(&raw[start..end]) {
            Ok(p) => p,
            Err(e) => {
                return Err(error::ErrorKind::EncryptionError(format!(
                    "Could not reinstate key {:?}",
                    e
                ))
                .into());
            }
        };
        let pubkey = match private.pub_as_raw() {
            Ok(v) => v,
            Err(e) => {
                return Err(error::ErrorKind::EncryptionError(format!(
                    "Could not dump public key: {:?}",
                    e
                ))
                .into());
            }
        };
        Ok(Key {
            private,
            public: pubkey,
            auth: auth.to_vec(),
        })
    }
}

pub trait Cryptography {
    /// generate a new local EC p256 key
    fn generate_key() -> error::Result<Key>;

    /// create a test key for testing
    fn test_key(priv_key: &str, pub_key: &str, auth: &str) -> Key;

    /// General decrypt function. Calls to decrypt_aesgcm or decrypt_aes128gcm as needed.
    // (sigh, can't use notifier::Notification because of circular dependencies.)
    fn decrypt(
        key: &Key,
        body: &str,
        encoding: &str,
        salt: Option<&str>,
        dh: Option<&str>,
    ) -> error::Result<Decrypted>;
    // IIUC: objects created on one side of FFI can't be freed on the other side, so we have to use references (or clone)

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

pub struct Crypto;

pub fn get_bytes(size: usize) -> error::Result<Vec<u8>> {
    let mut bytes = vec![0u8; size];
    rand_bytes(bytes.as_mut_slice())?;
    Ok(bytes)
}

/// Extract the sub-value from the header.
/// Sub values have the form of `label=value`. Due to a bug in some push providers, treat ',' and ';' as
/// equivalent.
/// @param string: the string to search,
fn extract_value(string: Option<&str>, target: &str) -> Option<Vec<u8>> {
    if let Some(val) = string {
        if !val.contains(&format!("{}=", target)) {
            log::debug!("No sub-value found for {}", target);
            return None;
        }
        let items: Vec<&str> = val.split(|c| c == ',' || c == ';').collect();
        for item in items {
            let kv: Vec<&str> = item.split('=').collect();
            if kv[0] == target {
                return match base64::decode_config(kv[1], base64::URL_SAFE_NO_PAD) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        log::error!("base64 failed for target:{}; {:?}", target, e);
                        None
                    }
                };
            }
        }
    }
    None
}

impl Cryptography for Crypto {
    /// Generate a new cryptographic Key
    fn generate_key() -> error::Result<Key> {
        let key = match LocalKeyPairImpl::generate_random() {
            Ok(k) => k,
            Err(e) => {
                return Err(error::ErrorKind::EncryptionError(format!(
                    "Could not generate key: {:?}",
                    e
                ))
                .into());
            }
        };
        let auth = get_bytes(SER_AUTH_LENGTH)?;
        let public = match key.pub_as_raw() {
            Ok(v) => v,
            Err(e) => {
                return Err(error::ErrorKind::EncryptionError(format!(
                    "Could not dump public key: {:?}",
                    e
                ))
                .into());
            }
        };
        Ok(Key {
            private: key,
            public,
            auth,
        })
    }

    // generate unit test key
    fn test_key(priv_key: &str, pub_key: &str, auth: &str) -> Key {
        let private = EcKey::private_key_from_der(
            &base64::decode_config(priv_key, base64::URL_SAFE_NO_PAD).unwrap(),
        )
        .unwrap();
        let public = base64::decode_config(pub_key, base64::URL_SAFE_NO_PAD).unwrap();
        let auth = base64::decode_config(auth, base64::URL_SAFE_NO_PAD).unwrap();

        Key {
            private: private.into(),
            public,
            auth,
        }
    }

    /// Decrypt the incoming webpush message based on the content-encoding
    fn decrypt(
        key: &Key,
        body: &str,
        encoding: &str,
        salt: Option<&str>,
        dh: Option<&str>,
    ) -> error::Result<Decrypted> {
        // convert the private key into something useful.
        let d_salt = extract_value(salt, "salt");
        let d_dh = extract_value(dh, "dh");
        let d_body = base64::decode_config(body, base64::URL_SAFE_NO_PAD).map_err(|e| {
            error::ErrorKind::TranscodingError(format!("Could not parse incoming body: {:?}", e))
        })?;

        match encoding.to_lowercase().as_str() {
            "aesgcm" => Self::decrypt_aesgcm(&key, &d_body, d_salt, d_dh),
            "aes128gcm" => Self::decrypt_aes128gcm(&key, &d_body),
            _ => Err(
                error::ErrorKind::EncryptionError("Unknown Content Encoding".to_string()).into(),
            ),
        }
    }

    // IIUC: objects created on one side of FFI can't be freed on the other side, so we have to use references (or clone)
    fn decrypt_aesgcm(
        key: &Key,
        content: &[u8],
        salt: Option<Vec<u8>>,
        crypto_key: Option<Vec<u8>>,
    ) -> error::Result<Decrypted> {
        let dh = match crypto_key {
            Some(v) => v,
            None => {
                return Err(
                    error::ErrorKind::EncryptionError("Missing public key".to_string()).into(),
                );
            }
        };
        let salt = match salt {
            Some(v) => v,
            None => {
                return Err(error::ErrorKind::EncryptionError("Missing salt".to_string()).into());
            }
        };
        let block = match AesGcmEncryptedBlock::new(&dh, &salt, 4096, content.to_vec()) {
            Ok(b) => b,
            Err(e) => {
                return Err(error::ErrorKind::EncryptionError(format!(
                    "Could not create block: {}",
                    e
                ))
                .into());
            }
        };
        match AesGcmEceWebPushImpl::decrypt(&key.private, &key.auth, &block) {
            Ok(result) => Ok(result),
            Err(e) => Err(error::ErrorKind::OpenSSLError(format!("{:?}", e)).into()),
        }
    }

    fn decrypt_aes128gcm(key: &Key, content: &[u8]) -> error::Result<Vec<u8>> {
        match Aes128GcmEceWebPushImpl::decrypt(&key.private, &key.auth, &content) {
            Ok(result) => Ok(result),
            Err(e) => Err(error::ErrorKind::OpenSSLError(format!("{:?}", e)).into()),
        }
    }
}

#[cfg(test)]
mod crypto_tests {
    use super::*;

    use error;

    const PLAINTEXT:&str = "Amidst the mists and coldest frosts I thrust my fists against the\nposts and still demand to see the ghosts.\n\n";

    fn decrypter(
        ciphertext: &str,
        encoding: &str,
        salt: Option<&str>,
        dh: Option<&str>,
    ) -> error::Result<Vec<u8>> {
        // The following come from internal storage;
        // More than likely, this will be stored either as an encoded or raw DER.
        let priv_key_der_raw =
            "MHcCAQEEIKiZMcVhlVccuwSr62jWN4YPBrPmPKotJUWl1id0d2ifoAoGCCqGSM49AwEHoUQDQgAEFwl1-\
             zUa0zLKYVO23LqUgZZEVesS0k_jQN_SA69ENHgPwIpWCoTq-VhHu0JiSwhF0oPUzEM-FBWYoufO6J97nQ";
        // The auth token
        let auth_raw = "LsuUOBKVQRY6-l7_Ajo-Ag";
        // This would be the public key sent to the subscription service.
        let pub_key_raw = "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50";

        let key = Crypto::test_key(priv_key_der_raw, pub_key_raw, auth_raw);
        Crypto::decrypt(&key, ciphertext, encoding, salt, dh)
    }

    #[test]
    fn test_decrypt_aesgcm() {
        // The following comes from the delivered message body
        let ciphertext = "BNKu5uTFhjyS-06eECU9-6O61int3Rr7ARbm-xPhFuyDO5sfxVs-HywGaVonvzkarvfvXE9IRT_YNA81Og2uSqDasdMuw\
                          qm1zd0O3f7049IkQep3RJ2pEZTy5DqvI7kwMLDLzea9nroq3EMH5hYhvQtQgtKXeWieEL_3yVDQVg";
        // and now from the header values
        let dh = "keyid=foo;dh=BMOebOMWSRisAhWpRK9ZPszJC8BL9MiWvLZBoBU6pG6Kh6vUFSW4BHFMh0b83xCg3_7IgfQZXwmVuyu27vwiv5c,otherval=abcde";
        let salt = "salt=tSf2qu43C9BD0zkvRW5eUg";

        // and this is what it should be.

        let decrypted = decrypter(ciphertext, "aesgcm", Some(salt), Some(dh)).unwrap();

        assert_eq!(String::from_utf8(decrypted).unwrap(), PLAINTEXT.to_string());
    }

    #[test]
    fn test_fail_decrypt_aesgcm() {
        let ciphertext = "BNKu5uTFhjyS-06eECU9-6O61int3Rr7ARbm-xPhFuyDO5sfxVs-HywGaVonvzkarvfvXE9IRT_\
                          YNA81Og2uSqDasdMuwqm1zd0O3f7049IkQep3RJ2pEZTy5DqvI7kwMLDLzea9nroq3EMH5hYhvQtQgtKXeWieEL_3yVDQVg";
        let dh = "dh=BMOebOMWSRisAhWpRK9ZPszJC8BL9MiWvLZBoBU6pG6Kh6vUFSW4BHFMh0b83xCg3_7IgfQZXwmVuyu27vwiv5c";
        let salt = "salt=SomeInvalidSaltValue";

        decrypter(ciphertext, "aesgcm", Some(salt), Some(dh))
            .expect_err("Failed to abort, bad salt");
    }

    #[test]
    fn test_decrypt_aes128gcm() {
        let ciphertext =
            "Ek7iQgliMqS9kjFoiVOqRgAAEABBBFirfBtF6XTeHVPABFDveb1iu7uO1XVA_MYJeAo-\
             4ih8WYUsXSTIYmkKMv5_UB3tZuQI7BQ2EVpYYQfvOCrWZVMRL8fJCuB5wVXcoRoTaFJw\
             TlJ5hnw6IMSiaMqGVlc8drX7Hzy-ugzzAKRhGPV2x-gdsp58DZh9Ww5vHpHyT1xwVkXz\
             x3KTyeBZu4gl_zR0Q00li17g0xGsE6Dg3xlkKEmaalgyUyObl6_a8RA6Ko1Rc6RhAy2jdyY1LQbBUnA";

        let decrypted = decrypter(ciphertext, "aes128gcm", None, None).unwrap();
        assert_eq!(String::from_utf8(decrypted).unwrap(), PLAINTEXT.to_string());
    }

    #[test]
    fn test_key_serde() {
        let key = Crypto::generate_key().unwrap();
        let key_dump = key.serialize().unwrap();
        let key2 = Key::deserialize(key_dump).unwrap();
        assert!(key.private.to_raw() == key2.private.to_raw());
        assert!(key.public == key2.public);
        assert!(key.auth == key2.auth);
        assert!(key == key2);
    }

    #[test]
    fn test_key_debug() {
        let key = Crypto::generate_key().unwrap();

        println!("Key: {:?}", key);
    }
}
