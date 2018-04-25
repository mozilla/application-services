/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error::{Result, ErrorKind};
use base64;
use openssl::{self, symm};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use std::fmt::Write;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct KeyBundle {
    enc_key: Vec<u8>,
    mac_key: Vec<u8>,
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        // There's no way for this unwrap not to work.
        write!(&mut result, "{:02x}", byte).unwrap();
    }
    result
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
        let enc_bytes = base64::decode(&enc)?;
        let mac_bytes = base64::decode(&mac)?;
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

    #[inline]
    pub fn to_b64_array(&self) -> [String; 2] {
        [base64::encode(&self.enc_key), base64::encode(&self.mac_key)]
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

    pub fn hmac_string(&self, ciphertext: &[u8]) -> Result<String> {
        Ok(bytes_to_hex(&self.hmac(ciphertext)?))
    }

    pub fn verify_hmac(&self, expected_hmac: &[u8], ciphertext_base64: &str) -> Result<bool> {
        let computed_hmac = self.hmac(ciphertext_base64.as_bytes())?;
        // I suspect this is unnecessary for our case, but the rust-openssl docs
        // want us to use this over == to avoid sidechannels, and who am I to argue?
        Ok(openssl::memcmp::eq(&expected_hmac, &computed_hmac))
    }

    pub fn verify_hmac_string(&self, expected_hmac: &str, ciphertext_base64: &str) -> Result<bool> {
        let computed_hmac = self.hmac(ciphertext_base64.as_bytes())?;
        let computed_hmac_string = bytes_to_hex(&computed_hmac);
        Ok(openssl::memcmp::eq(&expected_hmac.as_bytes(), &computed_hmac_string.as_bytes()))
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

#[cfg(test)]
mod test {
    use super::*;

    static HMAC_B16: &'static str = "b1e6c18ac30deb70236bc0d65a46f7a4dce3b8b0e02cf92182b914e3afa5eebc";
    static IV_B64: &'static str = "GX8L37AAb2FZJMzIoXlX8w==";
    static HMAC_KEY_B64: &'static str = "MMntEfutgLTc8FlTLQFms8/xMPmCldqPlq/QQXEjx70=";
    static ENC_KEY_B64: &'static str ="9K/wLdXdw+nrTtXo4ZpECyHFNr4d7aYHqeg3KW9+m6Q=";

    static CIPHERTEXT_B64_PIECES: &'static [&'static str] = &[
        "NMsdnRulLwQsVcwxKW9XwaUe7ouJk5Wn80QhbD80l0HEcZGCynh45qIbeYBik0lgcHbK",
        "mlIxTJNwU+OeqipN+/j7MqhjKOGIlvbpiPQQLC6/ffF2vbzL0nzMUuSyvaQzyGGkSYM2",
        "xUFt06aNivoQTvU2GgGmUK6MvadoY38hhW2LCMkoZcNfgCqJ26lO1O0sEO6zHsk3IVz6",
        "vsKiJ2Hq6VCo7hu123wNegmujHWQSGyf8JeudZjKzfi0OFRRvvm4QAKyBWf0MgrW1F8S",
        "FDnVfkq8amCB7NhdwhgLWbN+21NitNwWYknoEWe1m6hmGZDgDT32uxzWxCV8QqqrpH/Z",
        "ggViEr9uMgoy4lYaWqP7G5WKvvechc62aqnsNEYhH26A5QgzmlNyvB+KPFvPsYzxDnSC",
        "jOoRSLx7GG86wT59QZw="
    ];

    static CLEARTEXT_B64_PIECES: &'static [&'static str] = &[
        "eyJpZCI6IjVxUnNnWFdSSlpYciIsImhpc3RVcmkiOiJmaWxlOi8vL1VzZXJzL2phc29u",
        "L0xpYnJhcnkvQXBwbGljYXRpb24lMjBTdXBwb3J0L0ZpcmVmb3gvUHJvZmlsZXMva3Nn",
        "ZDd3cGsuTG9jYWxTeW5jU2VydmVyL3dlYXZlL2xvZ3MvIiwidGl0bGUiOiJJbmRleCBv",
        "ZiBmaWxlOi8vL1VzZXJzL2phc29uL0xpYnJhcnkvQXBwbGljYXRpb24gU3VwcG9ydC9G",
        "aXJlZm94L1Byb2ZpbGVzL2tzZ2Q3d3BrLkxvY2FsU3luY1NlcnZlci93ZWF2ZS9sb2dz",
        "LyIsInZpc2l0cyI6W3siZGF0ZSI6MTMxOTE0OTAxMjM3MjQyNSwidHlwZSI6MX1dfQ=="
    ];

    #[test]
    fn test_hmac() {
        let key_bundle = KeyBundle::from_base64(ENC_KEY_B64, HMAC_KEY_B64).unwrap();
        let ciphertext_base64 = CIPHERTEXT_B64_PIECES.join("");
        let hmac = key_bundle.hmac_string(ciphertext_base64.as_bytes()).unwrap();
        assert_eq!(hmac, HMAC_B16);
        assert!(key_bundle.verify_hmac_string(HMAC_B16, &ciphertext_base64).unwrap());
    }

    #[test]
    fn test_decrypt() {
        let key_bundle = KeyBundle::from_base64(ENC_KEY_B64, HMAC_KEY_B64).unwrap();
        let ciphertext = base64::decode(&CIPHERTEXT_B64_PIECES.join("")).unwrap();
        let iv = base64::decode(IV_B64).unwrap();
        let s = key_bundle.decrypt(&ciphertext, &iv).unwrap();

        let cleartext = String::from_utf8(
            base64::decode(&CLEARTEXT_B64_PIECES.join("")).unwrap()).unwrap();
        assert_eq!(&cleartext, &s);
    }

    #[test]
    fn test_encrypt() {
        let key_bundle = KeyBundle::from_base64(ENC_KEY_B64, HMAC_KEY_B64).unwrap();
        let iv = base64::decode(IV_B64).unwrap();

        let cleartext_bytes = base64::decode(&CLEARTEXT_B64_PIECES.join("")).unwrap();
        let encrypted_bytes = key_bundle.encrypt_bytes_with_iv(&cleartext_bytes, &iv).unwrap();

        let expect_ciphertext = base64::decode(&CIPHERTEXT_B64_PIECES.join("")).unwrap();

        assert_eq!(&encrypted_bytes, &expect_ciphertext);

        let (enc_bytes2, iv2) = key_bundle.encrypt_bytes_rand_iv(&cleartext_bytes).unwrap();
        assert_ne!(&enc_bytes2, &expect_ciphertext);

        let s = key_bundle.decrypt(&enc_bytes2, &iv2).unwrap();
        assert_eq!(&cleartext_bytes, &s.as_bytes());
    }
}
