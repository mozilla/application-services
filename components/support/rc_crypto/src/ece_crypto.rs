/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    aead,
    agreement::{self, Curve, EcKey, UnparsedPublicKey},
    digest, hkdf, hmac, rand,
};
use ece::crypto::{Cryptographer, EcKeyComponents, LocalKeyPair, RemotePublicKey};

impl From<crate::Error> for ece::Error {
    fn from(_: crate::Error) -> Self {
        ece::Error::CryptoError
    }
}

pub struct RcCryptoLocalKeyPair {
    wrapped: agreement::KeyPair<agreement::Static>,
}
// SECKEYPrivateKeyStr and SECKEYPublicKeyStr are Sync.
unsafe impl Sync for RcCryptoLocalKeyPair {}

impl RcCryptoLocalKeyPair {
    pub fn from_raw_components(components: &EcKeyComponents) -> Result<Self, ece::Error> {
        let ec_key = EcKey::new(
            Curve::P256,
            components.private_key(),
            components.public_key(),
        );
        let priv_key = agreement::PrivateKey::<agreement::Static>::import(&ec_key)?;
        let wrapped = agreement::KeyPair::<agreement::Static>::from_private_key(priv_key)?;
        Ok(RcCryptoLocalKeyPair { wrapped })
    }

    pub fn generate_random() -> Result<Self, ece::Error> {
        let wrapped = agreement::KeyPair::<agreement::Static>::generate(&agreement::ECDH_P256)?;
        Ok(RcCryptoLocalKeyPair { wrapped })
    }

    fn agree(&self, peer: &RcCryptoRemotePublicKey) -> Result<Vec<u8>, ece::Error> {
        let peer_public_key_raw_bytes = &peer.as_raw()?;
        let peer_public_key =
            UnparsedPublicKey::new(&agreement::ECDH_P256, &peer_public_key_raw_bytes);
        self.wrapped
            .private_key()
            .agree_static(&peer_public_key)?
            .derive(|z| Ok(z.to_vec()))
    }
}

impl LocalKeyPair for RcCryptoLocalKeyPair {
    fn raw_components(&self) -> Result<EcKeyComponents, ece::Error> {
        let ec_key = self.wrapped.private_key().export()?;
        Ok(EcKeyComponents::new(
            ec_key.private_key(),
            ec_key.public_key(),
        ))
    }

    fn pub_as_raw(&self) -> Result<Vec<u8>, ece::Error> {
        let bytes = self.wrapped.public_key().to_bytes()?;
        Ok(bytes.to_vec())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
pub struct RcCryptoRemotePublicKey {
    raw: Vec<u8>,
}

impl RcCryptoRemotePublicKey {
    pub fn from_raw(bytes: &[u8]) -> Result<RcCryptoRemotePublicKey, ece::Error> {
        Ok(RcCryptoRemotePublicKey {
            raw: bytes.to_owned(),
        })
    }
}

impl RemotePublicKey for RcCryptoRemotePublicKey {
    fn as_raw(&self) -> Result<Vec<u8>, ece::Error> {
        Ok(self.raw.to_vec())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct RcCryptoCryptographer;

impl Cryptographer for RcCryptoCryptographer {
    fn generate_ephemeral_keypair(&self) -> Result<Box<dyn LocalKeyPair>, ece::Error> {
        Ok(Box::new(RcCryptoLocalKeyPair::generate_random()?))
    }

    fn import_key_pair(
        &self,
        components: &EcKeyComponents,
    ) -> Result<Box<dyn LocalKeyPair>, ece::Error> {
        Ok(Box::new(RcCryptoLocalKeyPair::from_raw_components(
            components,
        )?))
    }

    fn import_public_key(&self, raw: &[u8]) -> Result<Box<dyn RemotePublicKey>, ece::Error> {
        Ok(Box::new(RcCryptoRemotePublicKey::from_raw(raw)?))
    }

    fn compute_ecdh_secret(
        &self,
        remote: &dyn RemotePublicKey,
        local: &dyn LocalKeyPair,
    ) -> Result<Vec<u8>, ece::Error> {
        let local_any = local.as_any();
        let local = local_any.downcast_ref::<RcCryptoLocalKeyPair>().unwrap();
        let remote_any = remote.as_any();
        let remote = remote_any
            .downcast_ref::<RcCryptoRemotePublicKey>()
            .unwrap();
        local.agree(&remote)
    }

    fn hkdf_sha256(
        &self,
        salt: &[u8],
        secret: &[u8],
        info: &[u8],
        len: usize,
    ) -> Result<Vec<u8>, ece::Error> {
        let salt = hmac::SigningKey::new(&digest::SHA256, &salt);
        let mut out = vec![0u8; len];
        hkdf::extract_and_expand(&salt, &secret, &info, &mut out)?;
        Ok(out)
    }

    fn aes_gcm_128_encrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        data: &[u8],
    ) -> Result<Vec<u8>, ece::Error> {
        let key = aead::SealingKey::new(&aead::AES_128_GCM, key)?;
        let nonce = aead::Nonce::try_assume_unique_for_key(&aead::AES_128_GCM, iv)?;
        Ok(aead::seal(&key, nonce, aead::Aad::empty(), data)?)
    }

    fn aes_gcm_128_decrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        ciphertext_and_tag: &[u8],
    ) -> Result<Vec<u8>, ece::Error> {
        let key = aead::OpeningKey::new(&aead::AES_128_GCM, key)?;
        let nonce = aead::Nonce::try_assume_unique_for_key(&aead::AES_128_GCM, iv)?;
        Ok(aead::open(
            &key,
            nonce,
            aead::Aad::empty(),
            &ciphertext_and_tag,
        )?)
    }

    fn random_bytes(&self, dest: &mut [u8]) -> Result<(), ece::Error> {
        Ok(rand::fill(dest)?)
    }
}

// Please call `rc_crypto::ensure_initialized()` instead of calling
// this function directly.
pub(crate) fn init() {
    ece::crypto::set_cryptographer(&crate::ece_crypto::RcCryptoCryptographer)
        .expect("Failed to initialize `ece` cryptographer!")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cryptographer_backend() {
        crate::ensure_initialized();
        ece::crypto::test_cryptographer(RcCryptoCryptographer);
    }
}
