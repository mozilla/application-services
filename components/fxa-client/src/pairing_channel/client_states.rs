/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bytes::{Buf, BufMut, BytesMut};

use super::extension::{
    Extension, PreSharedKeyExtension, PskKeyExchangeExtension, SupportedVersionsExtension,
};
use super::key_schedule::KeySchedule;
use super::messages::{ClientHello, Finished, HandshakeMessage, HandshakeType};
use super::record_layer::{RecordLayer, RecordType};
use super::utils::{HASH_LENGTH, PRE_SHARED_KEY_ID, PSK_MODE_KE, VERSION_TLS_1_3};
use anyhow::Result;
use rc_crypto::{digest::Algorithm, hmac::SigningKey, rand};
//
// Client side State-machine for TLS Handshake Management.
//
// Internally, we manage the TLS connection by explicitly modelling the
// client and server state-machines from RFC8446.  You can think of
// these `State` objects as little plugins for the `Connection` class
// that provide different behaviours of `send` and `receive` depending
// on the state of the connection.
//
pub enum ClientState<F: FnMut(&[u8]) -> Result<()>> {
    Invalid,
    ClientWaitEe(ClientWaitEe<F>),
    ClientWaitSh(ClientWaitSh<F>),
    ClientWaitFinished(ClientWaitFinished<F>),
    ClientConnected(ClientConnected<F>),
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientState<F> {
    #[allow(dead_code)]
    pub fn client_init(psk: Vec<u8>, psk_id: Vec<u8>, callback: F) -> Result<Self> {
        let client_start = ClientStart {
            key_schedule: KeySchedule::new(),
            record_layer: RecordLayer::new(callback),
        };
        let client_start = client_start.initialize(&psk, &psk_id)?;
        Ok(ClientState::ClientWaitSh(client_start))
    }

    pub fn recv(&mut self, data: &[u8]) -> Result<(RecordType, Vec<u8>)> {
        match self {
            ClientState::ClientWaitEe(inner) => inner.record_layer.recv(data),
            ClientState::ClientWaitSh(inner) => inner.record_layer.recv(data),
            ClientState::ClientWaitFinished(inner) => inner.record_layer.recv(data),
            ClientState::ClientConnected(inner) => inner.record_layer.recv(data),
            _ => anyhow::bail!("Invalid state"),
        }
    }

    pub fn recv_application_data(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            ClientState::ClientConnected(inner) => Ok(inner.recv_application_data(data)),
            _ => anyhow::bail!("State not connected"),
        }
    }

    pub fn recv_change_cipher_spec(&mut self, data: &[u8]) -> Result<()> {
        match self {
            ClientState::ClientWaitEe(inner) => inner.recv_change_cipher_spec(data),
            _ => anyhow::bail!("Invalid state"),
        }
    }

    pub fn add_to_transcript(&mut self, data: &[u8]) -> Result<()> {
        match self {
            ClientState::ClientWaitEe(inner) => inner.key_schedule.add_to_transcript(data),
            ClientState::ClientWaitSh(inner) => inner.key_schedule.add_to_transcript(data),
            ClientState::ClientWaitFinished(inner) => inner.key_schedule.add_to_transcript(data),
            ClientState::ClientConnected(inner) => inner.key_schedule.add_to_transcript(data),
            _ => anyhow::bail!("Invalid state"),
        };
        Ok(())
    }

    pub fn recv_handshake_message(self, message_bytes: &[u8]) -> Result<Self> {
        let msg = HandshakeMessage::from_bytes(message_bytes)?;
        match self {
            ClientState::ClientWaitSh(inner) => Ok(ClientState::ClientWaitEe(
                inner.recv_handshake_message(msg)?,
            )),
            ClientState::ClientWaitEe(inner) => Ok(ClientState::ClientWaitFinished(
                inner.recv_handshake_message(msg)?,
            )),
            ClientState::ClientWaitFinished(inner) => Ok(ClientState::ClientConnected(
                inner.recv_handshake_message(msg)?,
            )),
            _ => Ok(self),
        }
    }
}

// These states implement (part of) the client state-machine from
// https://tools.ietf.org/html/rfc8446#appendix-A.1
//
// Since we're only implementing a small subset of TLS1.3,
// we only need a small subset of the handshake.  It basically goes:
//
//   * send ClientHello
//   * receive ServerHello
//   * receive EncryptedExtensions
//   * receive server Finished
//   * send client Finished
//
// We include some unused states for completeness, so that it's easier
// to check the implementation against the diagrams in the RFC.
pub struct ClientStart<F: FnMut(&[u8]) -> Result<()>> {
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientStart<F> {
    #[allow(dead_code)]
    fn new(key_schedule: KeySchedule, record_layer: RecordLayer<F>) -> Self {
        Self {
            key_schedule,
            record_layer,
        }
    }
    pub fn initialize(mut self, psk: &[u8], psk_id: &[u8]) -> Result<ClientWaitSh<F>> {
        self.record_layer.flush()?;
        self.key_schedule.add_psk(psk)?;
        // Construct a ClientHello message with our single PSK.
        // We can't know the PSK binder value yet, so we initially write zeros.
        let mut random_bytes = vec![0u8; 32];
        let mut session_id = vec![0u8; 32];
        rand::fill(&mut random_bytes)?;
        // Random legacy_session_id; we *could* send an empty string here,
        // but sending a random one makes it easier to be compatible with
        // the data emitted by tlslite-ng for test-case generation.
        rand::fill(&mut session_id)?;
        let versions = vec![VERSION_TLS_1_3];
        let supported_version_ext: Extension =
            SupportedVersionsExtension::from_versions(versions).into();
        let modes = vec![PSK_MODE_KE];
        let psk_key_exchange_ext: Extension = PskKeyExchangeExtension::new(modes).into();
        let identities = vec![psk_id.to_vec()];
        let binders = vec![vec![0u8; HASH_LENGTH]];
        let pre_shared_key_ext: Extension =
            PreSharedKeyExtension::from_identities_and_binders(identities, binders).into();
        let mut extensions = Vec::new();
        extensions.push(supported_version_ext);
        extensions.push(psk_key_exchange_ext);
        extensions.push(pre_shared_key_ext);
        let client_hello_msg: HandshakeMessage =
            ClientHello::new(random_bytes, session_id.clone(), extensions).into();
        let mut buf = BytesMut::with_capacity(1024);
        client_hello_msg.write(&mut buf)?;
        // Now that we know what the ClientHello looks like,
        // go back and calculate the appropriate PSK binder value.
        // We only support a single PSK, so the length of the binders field is the
        // length of the hash plus one for rendering it as a variable-length byte array,
        // plus two for rendering the variable-length list of PSK binders.
        let psk_binders_size = HASH_LENGTH + 1 + 2;
        let truncated_transcript = buf
            .get(0..buf.len() - psk_binders_size)
            .ok_or_else(|| anyhow::Error::msg("Invalid subslice length"))?;
        let ext_bind_key = self.key_schedule.get_ext_bind_key()?;
        let signing_key = SigningKey::new(&Algorithm::SHA256, &ext_bind_key);
        let psk_binder = self
            .key_schedule
            .calculate_finished_mac(&signing_key, truncated_transcript)?;
        let mut actual_buf = BytesMut::with_capacity(1024);
        actual_buf.put(
            buf.get(0..buf.len() - HASH_LENGTH)
                .ok_or_else(|| anyhow::Error::msg("Invalid subslice length"))?,
        );
        actual_buf.put(psk_binder.as_slice());
        let actual_buf = actual_buf.freeze();
        let actual_buf = actual_buf.bytes();
        self.key_schedule.add_to_transcript(actual_buf);
        self.record_layer.send(RecordType::Handshake, actual_buf)?;
        let client_wait_sh: ClientWaitSh<F> = self.into();
        Ok(client_wait_sh.initialize(session_id)?)
    }
}

pub struct ClientWaitSh<F: FnMut(&[u8]) -> Result<()>> {
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
    session_id: Vec<u8>,
}
impl<F: FnMut(&[u8]) -> Result<()>> From<ClientStart<F>> for ClientWaitSh<F> {
    fn from(client_start: ClientStart<F>) -> Self {
        Self {
            key_schedule: client_start.key_schedule,
            record_layer: client_start.record_layer,
            session_id: Vec::new(),
        }
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientWaitSh<F> {
    fn initialize(mut self, session_id: Vec<u8>) -> Result<Self> {
        self.record_layer.flush()?;
        self.session_id = session_id;
        Ok(self)
    }

    fn recv_handshake_message(mut self, message: HandshakeMessage) -> Result<ClientWaitEe<F>> {
        if *message.get_type() != HandshakeType::ServerHello {
            anyhow::bail!("Invalid message");
        }
        if hex::encode(&self.session_id) != hex::encode(message.get_session_id()) {
            anyhow::bail!("Invalid session id")
        }
        let psk_ext = message
            .get_extensions()
            .iter()
            .find(|ext| ext.get_type_tag() == PRE_SHARED_KEY_ID)
            .ok_or_else(|| anyhow::Error::msg("No psk extension!"))?;
        if psk_ext.get_selected_identity()? != 0 {
            anyhow::bail!("Invalid selected identity")
        }
        self.key_schedule.add_ecdhe(b"")?;
        let send_key = self.key_schedule.get_client_handshake_secret()?;
        let recv_key = self.key_schedule.get_server_handshake_secret()?;
        self.record_layer.set_send_key(&send_key)?;
        self.record_layer.set_recv_key(&recv_key)?;
        let client_wait_ee: ClientWaitEe<F> = self.into();
        Ok(client_wait_ee.initialize()?)
    }
}

pub struct ClientWaitEe<F: FnMut(&[u8]) -> Result<()>> {
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
    has_seen_change_cipher_spec: bool,
}
impl<F: FnMut(&[u8]) -> Result<()>> From<ClientWaitSh<F>> for ClientWaitEe<F> {
    fn from(client_wait_sh: ClientWaitSh<F>) -> Self {
        Self {
            key_schedule: client_wait_sh.key_schedule,
            record_layer: client_wait_sh.record_layer,
            has_seen_change_cipher_spec: false,
        }
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientWaitEe<F> {
    fn initialize(mut self) -> Result<Self> {
        self.record_layer.flush()?;
        Ok(self)
    }
    fn recv_change_cipher_spec(&mut self, bytes: &[u8]) -> Result<()> {
        if self.has_seen_change_cipher_spec {
            anyhow::bail!("Seen change cipher spec before!")
        }
        if bytes.len() != 1 || bytes[0] != 1 {
            anyhow::bail!("Invalid bytes fo change cipher spec")
        }
        self.has_seen_change_cipher_spec = true;
        Ok(())
    }

    fn recv_handshake_message(self, message: HandshakeMessage) -> Result<ClientWaitFinished<F>> {
        // We don't make use of any encrypted extensions, but we still
        // have to wait for the server to send the (empty) list of them.
        if *message.get_type() != HandshakeType::EncryptedExtensions {
            anyhow::bail!("invalid message");
        }
        // We do not support any EncryptedExtensions.
        if !message.get_extensions().is_empty() {
            anyhow::bail!("Unsupported extension")
        }
        let server_finished_transcript = self.key_schedule.get_transcript();
        let client_wait_finished: ClientWaitFinished<F> = self.into();
        Ok(client_wait_finished.initialize(server_finished_transcript)?)
    }
}

pub struct ClientWaitFinished<F: FnMut(&[u8]) -> Result<()>> {
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
    server_finished_transcript: Vec<u8>,
}

impl<F: FnMut(&[u8]) -> Result<()>> From<ClientWaitEe<F>> for ClientWaitFinished<F> {
    fn from(client_wait_ee: ClientWaitEe<F>) -> Self {
        Self {
            key_schedule: client_wait_ee.key_schedule,
            record_layer: client_wait_ee.record_layer,
            server_finished_transcript: Vec::new(),
        }
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientWaitFinished<F> {
    fn initialize(mut self, transcript: Vec<u8>) -> Result<Self> {
        self.record_layer.flush()?;
        self.server_finished_transcript = transcript;
        Ok(self)
    }

    fn send_handshake_mesage(&mut self, message: HandshakeMessage) -> Result<()> {
        let bytes = message.to_bytes()?;
        self.key_schedule.add_to_transcript(&bytes);
        self.record_layer.send(RecordType::Handshake, &bytes)?;
        Ok(())
    }

    fn recv_handshake_message(mut self, message: HandshakeMessage) -> Result<ClientConnected<F>> {
        if *message.get_type() != HandshakeType::Finished {
            anyhow::bail!("Inavlid message")
        }
        let server_secret = self.key_schedule.get_server_handshake_secret()?;
        let signing_key = SigningKey::new(&Algorithm::SHA256, &server_secret);
        self.key_schedule.verify_finished_mac(
            &signing_key,
            &message.get_verify_data(),
            &self.server_finished_transcript,
        )?;
        // Send our own Finished message in return.
        // This must be encrypted with the handshake traffic key,
        // but must not appear in the transcript used to calculate the application keys.
        let client_secret = self.key_schedule.get_client_handshake_secret()?;
        let signing_key = SigningKey::new(&Algorithm::SHA256, &client_secret);
        let client_finished_mac = self
            .key_schedule
            .calculate_finished_mac(&signing_key, &self.key_schedule.get_transcript())?;
        self.key_schedule.finalize()?;
        self.send_handshake_mesage(Finished::new(client_finished_mac).into())?;
        let client_secret = self.key_schedule.get_client_application_secret()?;
        let server_secret = self.key_schedule.get_server_application_secret()?;
        self.record_layer.set_send_key(&client_secret)?;
        // BIG TODO: Add checking the recieve buffer
        self.record_layer.set_recv_key(&server_secret)?;
        let client_connected: ClientConnected<F> = self.into();
        Ok(client_connected.initialize()?)
    }
}

pub struct ClientConnected<F: FnMut(&[u8]) -> Result<()>> {
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
}

impl<F: FnMut(&[u8]) -> Result<()>> From<ClientWaitFinished<F>> for ClientConnected<F> {
    fn from(client_wait_finished: ClientWaitFinished<F>) -> Self {
        Self {
            key_schedule: client_wait_finished.key_schedule,
            record_layer: client_wait_finished.record_layer,
        }
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> ClientConnected<F> {
    fn initialize(mut self) -> Result<Self> {
        self.record_layer.flush()?;
        Ok(self)
    }

    #[allow(dead_code)]
    pub fn send_application_data(&mut self, bytes: &[u8]) -> Result<()> {
        self.record_layer.send(RecordType::ApplicationData, bytes)?;
        self.record_layer.flush()?;
        Ok(())
    }

    fn recv_application_data(&self, bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PSK: &[u8] = b"aabbccddeeff";
    const PSK_ID: &[u8] = b"testkey";

    #[test]
    fn test_client_start() {
        let mut sent_items = Vec::new();
        let client_start = ClientState::client_init(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            sent_items.push(data.to_vec());
            Ok(())
        })
        .unwrap();
        if let ClientState::ClientWaitSh(_) = client_start {
            // Okay
        } else {
            panic!("Invalid state")
        }
    }
}
