/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const HASH_LENGTH: usize = 32;
use anyhow::Result;
use rc_crypto::{
    digest, hkdf, hmac,
    hmac::{SigningKey, VerificationKey},
};
enum Stage {
    Uninitialized,
    EarlySecret {
        // This tracks the main secret from with other keys are derived at each stage.
        secret: Vec<u8>,
        ext_binder_key: Vec<u8>,
    },
    HandshakeSecret {
        secret: Vec<u8>,
        client_secret: Vec<u8>,
        server_secret: Vec<u8>,
    },
    MasterSecret {
        client_secret: Vec<u8>,
        server_secret: Vec<u8>,
    },
}

/// The `KeySchedule` struct progresses through three stages corresponding
/// to the three phases of the TLS1.3 key schedule:
///
///   UNINITIALIZED
///       |
///       | add_psk()
///       v
///   EARLY_SECRET
///       |
///       | add_ecdhe()
///       v
///   HANDSHAKE_SECRET
///       |
///       | finalize()
///       v
///   MASTER_SECRET
///
/// It will error out if the calling code attempts to add key material
/// in the wrong order.
pub struct KeySchedule {
    stage: Stage,
    transcript: Vec<u8>,
}

impl KeySchedule {
    pub fn new() -> Self {
        Self {
            stage: Stage::Uninitialized,
            //TODO: Make a buff writer
            transcript: Vec::new(),
        }
    }

    pub fn get_transcript(&self) -> Vec<u8> {
        self.transcript.clone()
    }

    pub fn get_ext_bind_key(&self) -> Result<Vec<u8>> {
        if let Stage::EarlySecret {
            secret: _,
            ext_binder_key,
        } = &self.stage
        {
            return Ok(ext_binder_key.clone());
        }
        anyhow::bail!("Invalid state")
    }

    pub fn add_psk(&mut self, psk: &[u8]) -> Result<()> {
        // Use the selected PSK to calculate the early secret
        if let Stage::Uninitialized = &self.stage {
            let zeros = vec![0u8; HASH_LENGTH];
            let salt = SigningKey::new(&digest::Algorithm::SHA256, &zeros);
            let signing_key = hkdf::extract(&salt, psk)?;
            let empty = vec![0u8; 0];
            let ext_binder_key = self.derive_secret(&signing_key, "ext binder", &empty)?;
            let secret = self.derive_secret(&signing_key, "derived", &empty)?;
            // Do this last in case any errors happen
            self.stage = Stage::EarlySecret {
                secret,
                ext_binder_key,
            };
            return Ok(());
        }
        Err(anyhow::Error::msg("Invalid stage! should be uninitialized"))
    }

    pub fn add_ecdhe(&mut self, ecdhe: &[u8]) -> Result<()> {
        if let Stage::EarlySecret {
            secret,
            ext_binder_key: _,
        } = &self.stage
        {
            // Mix in the ECDHE output if any, use 32 zero bytes if none was given
            let mut ecdhe = ecdhe.to_vec();
            if ecdhe.is_empty() {
                ecdhe = vec![0u8; HASH_LENGTH];
            }
            let salt = SigningKey::new(&digest::Algorithm::SHA256, secret);
            let signing_key = hkdf::extract(&salt, &ecdhe)?;
            let client_secret =
                self.derive_secret(&signing_key, "c hs traffic", &self.transcript)?;
            let server_secret =
                self.derive_secret(&signing_key, "s hs traffic", &self.transcript)?;
            let secret = self.derive_secret(&signing_key, "derived", b"")?;
            self.stage = Stage::HandshakeSecret {
                secret,
                client_secret,
                server_secret,
            };
            return Ok(());
        }
        anyhow::bail!("Invalid stage! Should be EarlySecret");
    }

    pub fn get_client_handshake_secret(&self) -> Result<Vec<u8>> {
        if let Stage::HandshakeSecret {
            client_secret,
            server_secret: _,
            secret: _,
        } = &self.stage
        {
            Ok(client_secret.clone())
        } else {
            anyhow::bail!("Invalid stage")
        }
    }

    pub fn get_server_handshake_secret(&self) -> Result<Vec<u8>> {
        if let Stage::HandshakeSecret {
            client_secret: _,
            server_secret,
            secret: _,
        } = &self.stage
        {
            Ok(server_secret.clone())
        } else {
            anyhow::bail!("Invalid stage")
        }
    }

    pub fn finalize(&mut self) -> Result<()> {
        if let Stage::HandshakeSecret {
            secret,
            client_secret: _,
            server_secret: _,
        } = &self.stage
        {
            let salt = SigningKey::new(&digest::Algorithm::SHA256, secret);
            let zeros = vec![0u8; HASH_LENGTH];
            let signing_key = hkdf::extract(&salt, &zeros)?;
            let client_secret =
                self.derive_secret(&signing_key, "c ap traffic", &self.transcript)?;
            let server_secret =
                self.derive_secret(&signing_key, "s ap traffic", &self.transcript)?;
            self.stage = Stage::MasterSecret {
                client_secret,
                server_secret,
            };
            return Ok(());
        }
        anyhow::bail!("Invalid stage! should be handshake secret stage");
    }

    pub fn get_client_application_secret(&self) -> Result<Vec<u8>> {
        if let Stage::MasterSecret {
            client_secret,
            server_secret: _,
        } = &self.stage
        {
            Ok(client_secret.clone())
        } else {
            anyhow::bail!("Invalid stage")
        }
    }

    pub fn get_server_application_secret(&self) -> Result<Vec<u8>> {
        if let Stage::MasterSecret {
            client_secret: _,
            server_secret,
        } = &self.stage
        {
            Ok(server_secret.clone())
        } else {
            anyhow::bail!("Invalid stage")
        }
    }

    pub fn add_to_transcript(&mut self, bytes: &[u8]) {
        self.transcript.extend_from_slice(bytes);
    }

    pub fn calculate_finished_mac(
        &self,
        base_key: &SigningKey,
        transcript: &[u8],
    ) -> Result<Vec<u8>> {
        let empty = vec![0u8; 0];
        let finished_key = hkdf::expand_label(base_key, "finished", &empty, HASH_LENGTH)?;
        let finished_key = SigningKey::new(&digest::Algorithm::SHA256, &finished_key);
        let signiture = hmac::sign(
            &finished_key,
            &digest::digest(&digest::Algorithm::SHA256, transcript)?
                .as_ref()
                .to_vec(),
        )?;
        Ok(signiture.as_ref().into())
    }

    pub fn verify_finished_mac(
        &self,
        base_key: &SigningKey,
        mac: &[u8],
        transcript: &[u8],
    ) -> Result<()> {
        let empty = vec![0u8; 0];
        let finished_key = hkdf::expand_label(base_key, "finished", &empty, HASH_LENGTH)?;
        let finished_key = VerificationKey::new(&digest::Algorithm::SHA256, &finished_key);
        let transcript = digest::digest(&digest::Algorithm::SHA256, transcript)?;
        hmac::verify(&finished_key, transcript.as_ref(), mac)?;
        Ok(())
    }

    fn derive_secret(
        &self,
        signing_key: &SigningKey,
        label: &str,
        transcript: &[u8],
    ) -> Result<Vec<u8>> {
        let transcript_hash = digest::digest(&digest::Algorithm::SHA256, transcript)?;
        let res = hkdf::expand_label(signing_key, label, transcript_hash.as_ref(), HASH_LENGTH)?;
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PSK: &[u8] = b"aabbccddeeff";
    const KEYS_EXT_BINDER: &str =
        "573c05ab12932bd141a222c46db9172205c9f9d0c9326c42c5604eed55b57e3a";
    const KEYS_PLAINTEXT_TRANSCRIPT: &str = "fake plaintext transcript";
    const KEYS_CLIENT_HANDSHAKE_TRAFFIC_SECRET: &str =
        "d21e1d6279c57611c6e85e8390cb1676ed1a545da75bfa3853f128f77ea15196";
    const KEYS_SERVER_HANDSHAKE_TRAFFIC_SECRET: &str =
        "6f8923e53e434a4f34333b5c3ea60f21f90df3600eec82c588e4ebfe88273626";
    const KEYS_ENCRYPTED_TRANSCRIPT: &str = "fake encrypted transcript";
    const KEYS_CLIENT_APPLICATION_TRAFFIC_SECRET: &str =
        "65d7f3a53ec6e224c2594e4ef3729cb174137a97a22b0eb78f459fd0e5797fb7";
    const KEYS_SERVER_APPLICATION_TRAFFIC_SECRET: &str =
        "9ca237a625b861b84b15c0d0013fa6067618535ecf3b26e4f40580765863f8ea";

    #[test]
    fn test_ecdhe_before_psk() {
        let mut ks = KeySchedule::new();
        let ecdhe = vec![0u8; HASH_LENGTH];
        assert!(ks.add_ecdhe(&ecdhe).is_err());
    }

    #[test]
    fn test_finialize_before_psk() {
        let mut ks = KeySchedule::new();
        assert!(ks.finalize().is_err());
    }

    #[test]
    fn test_accept_psk() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        if let Stage::EarlySecret {
            secret: _,
            ext_binder_key,
        } = ks.stage
        {
            assert_eq!(hex::encode(ext_binder_key), KEYS_EXT_BINDER);
        } else {
            panic!("Unexpected stage, should be early secret");
        }
    }
    #[test]
    fn test_error_adding_psk_twice() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        assert!(ks.add_psk(PSK).is_err());
    }

    #[test]
    fn test_error_finalize_before_ecdhe() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        assert!(ks.finalize().is_err());
    }

    #[test]
    fn test_accepts_ecdhe_after_psk() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        if let Stage::HandshakeSecret {
            client_secret,
            server_secret,
            secret: _,
        } = ks.stage
        {
            assert_eq!(
                hex::encode(client_secret),
                KEYS_CLIENT_HANDSHAKE_TRAFFIC_SECRET
            );
            assert_eq!(
                hex::encode(server_secret),
                KEYS_SERVER_HANDSHAKE_TRAFFIC_SECRET
            );
        } else {
            panic!("Unexpected stage, should be in Handshake secret");
        }
    }

    #[test]
    fn test_error_adding_psk_after_ecdhe() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        assert!(ks.add_psk(PSK).is_err());
    }

    #[test]
    fn test_error_adding_ecdhe_twice() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        assert!(ks.add_ecdhe(&zeros).is_err());
    }

    #[test]
    fn test_accepts_finalize_after_ecdhe() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        ks.add_to_transcript(KEYS_ENCRYPTED_TRANSCRIPT.as_bytes());
        ks.finalize().unwrap();
        if let Stage::MasterSecret {
            client_secret,
            server_secret,
        } = ks.stage
        {
            assert_eq!(
                hex::encode(client_secret),
                KEYS_CLIENT_APPLICATION_TRAFFIC_SECRET
            );
            assert_eq!(
                hex::encode(server_secret),
                KEYS_SERVER_APPLICATION_TRAFFIC_SECRET
            );
        } else {
            panic!("Unexpected stage, should be MasterSecret stage");
        }
    }

    #[test]
    fn test_error_adding_psk_after_finalize() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        ks.add_to_transcript(KEYS_ENCRYPTED_TRANSCRIPT.as_bytes());
        ks.finalize().unwrap();
        assert!(ks.add_psk(PSK).is_err());
    }

    #[test]
    fn test_error_adding_ecdhe_after_finalize() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        ks.add_to_transcript(KEYS_ENCRYPTED_TRANSCRIPT.as_bytes());
        ks.finalize().unwrap();
        assert!(ks.add_ecdhe(&zeros).is_err());
    }

    #[test]
    fn test_error_finalize_twice() {
        let mut ks = KeySchedule::new();
        ks.add_psk(PSK).unwrap();
        ks.add_to_transcript(KEYS_PLAINTEXT_TRANSCRIPT.as_bytes());
        let zeros = vec![0u8; HASH_LENGTH];
        ks.add_ecdhe(&zeros).unwrap();
        ks.add_to_transcript(KEYS_ENCRYPTED_TRANSCRIPT.as_bytes());
        ks.finalize().unwrap();
        assert!(ks.finalize().is_err());
    }
}
