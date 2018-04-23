/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error::{Error, Result, ErrorKind};
use base64;
use openssl::{self, symm};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyBundle {
    enc_key: Vec<u8>,
    mac_key: Vec<u8>,
}

impl KeyBundle {

    /// Construct a key bundle from the already-decoded encrypt and hmac keys.
    /// Panics (asserts) if they aren't both 32 bytes.
    pub fn new(enc: Vec<u8>, mac: Vec<u8>) -> Result<KeyBundle> {
        if enc.len() != 32 {
            // We probably should say which is bad...
            return Err(ErrorKind::BadKeyLength("enc_key", enc.len()).into());
        }
        if mac.len() != 32 {
            return Err(ErrorKind::BadKeyLength("mac_key", mac.len()).into());
        }
        Ok(KeyBundle { enc_key: enc, mac_key: mac })
    }

    pub fn new_random() -> Result<KeyBundle> {
        let mut buffer = [0u8; 64];
        openssl::rand::rand_bytes(&mut buffer)?;
        KeyBundle::from_ksync_bytes(&buffer)
    }

    pub fn from_ksync_bytes(ksync: &[u8]) -> Result<KeyBundle> {
        if ksync.len() != 64 {
            return Err(ErrorKind::BadKeyLength("kSync", ksync.len()).into());
        }
        Ok(KeyBundle {
            enc_key: ksync[0..32].into(),
            mac_key: ksync[32..64].into()
        })
    }

    pub fn from_ksync_base64(ksync: &str) -> Result<KeyBundle> {
        let bytes = base64::decode_config(&ksync, base64::URL_SAFE_NO_PAD)?;
        KeyBundle::from_ksync_bytes(&bytes)
    }

    pub fn from_base64(enc: &str, mac: &str) -> Result<KeyBundle> {
        let enc_bytes = base64::decode_config(&enc, base64::URL_SAFE_NO_PAD)?;
        let mac_bytes = base64::decode_config(&mac, base64::URL_SAFE_NO_PAD)?;
        KeyBundle::new(enc_bytes.into(), mac_bytes.into())
    }

    #[inline]
    pub fn encryption_key(&self) -> &[u8] {
        &self.enc_key
    }

    #[inline]
    pub fn hmac_key(&self) -> &[u8] {
        &self.mac_key
    }

    /// Returns the 32 byte digest by value since it's small enough to be passed
    /// around cheaply, and easily convertable into a slice or vec if you want.
    fn hmac(&self, ciphertext: &[u8]) -> Result<[u8; 32]> {
        let mut out = [0u8; 32];
        let key = PKey::hmac(self.hmac_key())?;
        let mut signer = Signer::new(MessageDigest::sha256(), &key)?;
        signer.update(ciphertext)?;
        let size = signer.sign(&mut out)?;
        // This isn't an Err since it really should not be possible.
        assert!(size == 32, "Somehow the 256 bits from sha256 do not add up into 32 bytes...");
        Ok(out)
    }

    pub fn verify_hmac(&self, expected_hmac: &[u8], ciphertext_base64: &str) -> Result<bool> {
        let computed_hmac = self.hmac(ciphertext_base64.as_bytes())?;
        // I suspect this is unnecessary for our case, but the rust-openssl docs
        // want us to use this over == to avoid sidechannels, and who am I to argue?
        Ok(openssl::memcmp::eq(&expected_hmac, &computed_hmac))
    }

    /// Decrypt the provided ciphertext with the given iv, and decodes the
    /// result as a utf8 string.  Important: Caller must check verify_hmac first!
    pub fn decrypt(&self, ciphertext: &[u8], iv: &[u8]) -> Result<String> {
        let cleartext_bytes = symm::decrypt(symm::Cipher::aes_256_cbc(),
                                            self.encryption_key(),
                                            Some(iv),
                                            ciphertext)?;
        let cleartext = String::from_utf8(cleartext_bytes)?;
        Ok(cleartext)
    }

    /// Encrypt using the provided IV.
    pub fn encrypt_bytes_with_iv(&self, cleartext_bytes: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
        let ciphertext = symm::encrypt(symm::Cipher::aes_256_cbc(),
                                       self.encryption_key(),
                                       Some(iv),
                                       cleartext_bytes)?;
        Ok(ciphertext)
    }

    /// Generate a random iv and encrypt with it. Return both the encrypted bytes
    /// and the generated iv.
    pub fn encrypt_bytes_rand_iv(&self, cleartext_bytes: &[u8]) -> Result<(Vec<u8>, [u8; 16])> {
        let mut iv = [0u8; 16];
        openssl::rand::rand_bytes(&mut iv)?;
        let ciphertext = self.encrypt_bytes_with_iv(cleartext_bytes, &iv)?;
        Ok((ciphertext, iv))
    }

    pub fn encrypt_with_iv(&self, cleartext: &str, iv: &[u8]) -> Result<Vec<u8>> {
        self.encrypt_bytes_with_iv(cleartext.as_bytes(), iv)
    }

    pub fn encrypt_rand_iv(&self, cleartext: &str) -> Result<(Vec<u8>, [u8; 16])> {
        self.encrypt_bytes_rand_iv(cleartext.as_bytes())
    }
}


