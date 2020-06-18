/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::extension::{Extension, PreSharedKeyExtension, SupportedVersionsExtension};
use super::key_schedule::KeySchedule;
use super::messages::{EncryptedExtensions, Finished, HandshakeMessage, ServerHello};
use super::record_layer::{RecordLayer, RecordType};
use super::utils::{HASH_LENGTH, PRE_SHARED_KEY_ID, PSK_KEY_ID, PSK_MODE_KE, VERSION_TLS_1_3};
use anyhow::Result;
use rc_crypto::{digest::Algorithm, hmac::SigningKey, rand};

//
// Server side State-machine for TLS Handshake Management.
//
// Internally, we manage the TLS connection by explicitly modelling the
// client and server state-machines from RFC8446.  You can think of
// these `State` objects as little plugins for the `Connection` class
// that provide different behaviours of `send` and `receive` depending
// on the state of the connection.
//
pub enum ServerState<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    Invalid, // Used as a temporary value for mem::replace as we consume states
    ServerStart(ServerStart<F>),
    ServerWaitFinished(ServerWaitFinished<F>),
    ServerConnected(ServerConnected<F>),
}

impl<F> ServerState<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    pub fn server_init(psk: Vec<u8>, psk_id: Vec<u8>, callback: F) -> Result<Self> {
        let server_start = ServerStart {
            key_schedule: KeySchedule::new(),
            record_layer: RecordLayer::new(callback),
            psk,
            psk_id,
        };
        let server_start = server_start.initialize()?;
        Ok(ServerState::ServerStart(server_start))
    }

    pub fn recv(&mut self, data: &[u8]) -> Result<(RecordType, Vec<u8>)> {
        match self {
            ServerState::ServerStart(inner) => inner.record_layer.recv(data),
            ServerState::ServerWaitFinished(inner) => inner.record_layer.recv(data),
            ServerState::ServerConnected(inner) => inner.record_layer.recv(data),
            _ => anyhow::bail!("Invalid state"),
        }
    }

    pub fn recv_application_data(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            ServerState::ServerConnected(inner) => Ok(inner.recv_application_data(data)),
            _ => anyhow::bail!("State not connected"),
        }
    }

    pub fn add_to_transcript(&mut self, data: &[u8]) -> Result<()> {
        match self {
            ServerState::ServerStart(inner) => inner.key_schedule.add_to_transcript(data),
            ServerState::ServerWaitFinished(inner) => inner.key_schedule.add_to_transcript(data),
            ServerState::ServerConnected(inner) => inner.key_schedule.add_to_transcript(data),
            _ => anyhow::bail!("Invalid state"),
        };
        Ok(())
    }

    pub fn recv_handshake_message(self, message_bytes: &[u8]) -> Result<Self> {
        let msg = HandshakeMessage::from_bytes(message_bytes)?;
        match self {
            ServerState::ServerStart(inner) => Ok(ServerState::ServerWaitFinished(
                inner.recv_handshake_message(msg)?,
            )),
            ServerState::ServerWaitFinished(inner) => Ok(ServerState::ServerConnected(
                inner.recv_handshake_message(msg)?,
            )),
            _ => Ok(self),
        }
    }
}

// These states implement (part of) the server state-machine from
// https://tools.ietf.org/html/rfc8446#appendix-A.2
//
// Since we're only implementing a small subset of TLS1.3,
// we only need a small subset of the handshake.  It basically goes:
//
//   * receive ClientHello
//   * send ServerHello
//   * send empty EncryptedExtensions
//   * send server Finished
//   * receive client Finished
//
// We include some unused states for completeness, so that it's easier
// to check the implementation against the diagrams in the RFC.
pub struct ServerStart<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
    psk_id: Vec<u8>,
    psk: Vec<u8>,
}

impl<F> ServerStart<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn initialize(mut self) -> Result<Self> {
        self.record_layer.flush()?;
        Ok(self)
    }
    fn recv_handshake_message(mut self, msg: HandshakeMessage) -> Result<ServerWaitFinished<F>> {
        // In the spec, this is where we select connection parameters, and maybe
        // tell the client to try again if we can't find a compatible set.
        // Since we only support a fixed cipherset, the only thing to "negotiate"
        // is whether they provided an acceptable PSK.
        let psk_ext = msg
            .get_extensions()
            .iter()
            .find(|ext| ext.get_type_tag() == PRE_SHARED_KEY_ID);
        let psk_modes_ext = msg
            .get_extensions()
            .iter()
            .find(|ext| ext.get_type_tag() == PSK_KEY_ID);

        match (psk_ext, psk_modes_ext) {
            (Some(psk_ext), Some(psk_modes_ext)) => {
                let modes = psk_modes_ext.get_modes()?;
                if !modes.contains(&PSK_MODE_KE) {
                    anyhow::bail!("Invalid modes")
                }
                let identities = psk_ext.get_identities()?;
                let (idx, _val) = identities
                    .iter()
                    .enumerate()
                    .find(|(_i, identity)| *identity == &self.psk_id)
                    .ok_or_else(|| anyhow::Error::msg("Unknown PSK identity"))?;
                self.key_schedule.add_psk(&self.psk)?;
                let transcript = self.key_schedule.get_transcript();

                // Calculate size occupied by the PSK binders.
                let mut psk_binders_size = 2; // Vector16 representation overhead.
                let binders = psk_ext.get_binders()?; // Vector8 representation overhead.
                binders
                    .iter()
                    .for_each(|binder| psk_binders_size += binder.len() + 1);
                let ext_binder_key = self.key_schedule.get_ext_bind_key()?;
                let signing_key = SigningKey::new(&Algorithm::SHA256, &ext_binder_key);
                let mac = binders
                    .get(idx)
                    .ok_or_else(|| anyhow::Error::msg("Value does not exist"))?;
                let transcript = &transcript[..transcript.len() - psk_binders_size];
                self.key_schedule
                    .verify_finished_mac(&signing_key, mac, transcript)?;
                let server_negotiated: ServerNegotiated<F> = self.into();
                Ok(server_negotiated.initialize(msg.get_session_id(), idx)?)
            }
            _ => anyhow::bail!("Invalid message"),
        }
    }
}

pub struct ServerNegotiated<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
}

impl<F> From<ServerStart<F>> for ServerNegotiated<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn from(server_hello: ServerStart<F>) -> Self {
        Self {
            key_schedule: server_hello.key_schedule,
            record_layer: server_hello.record_layer,
        }
    }
}

impl<F> ServerNegotiated<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn initialize(mut self, session_id: Vec<u8>, idx: usize) -> Result<ServerWaitFinished<F>> {
        self.record_layer.flush()?;
        let mut random = vec![0u8; HASH_LENGTH];
        rand::fill(&mut random)?;
        let supported_versions: Extension =
            SupportedVersionsExtension::from_selected_version(VERSION_TLS_1_3).into();
        let pre_shared_key_ext: Extension =
            PreSharedKeyExtension::from_selected_identity(idx as u16).into();
        let mut extensions = Vec::new();
        extensions.push(supported_versions);
        extensions.push(pre_shared_key_ext);
        let server_hello: HandshakeMessage =
            ServerHello::new(random, session_id.clone(), extensions).into();
        self.send_handshake_message(server_hello)?;

        // If the client sent a non-empty sessionId, the server *must* send a change-cipher-spec for b/w compat.
        if !session_id.is_empty() {
            self.send_change_cipher_spec()?;
        }
        self.key_schedule.add_ecdhe(b"")?;
        let server_secret = self.key_schedule.get_server_handshake_secret()?;
        let client_secret = self.key_schedule.get_client_handshake_secret()?;
        self.record_layer.set_send_key(&server_secret)?;
        self.record_layer.set_recv_key(&client_secret)?;
        let encrypted_extension: HandshakeMessage = EncryptedExtensions::new(Vec::new()).into();
        // Send an empty EncryptedExtensions message.
        self.send_handshake_message(encrypted_extension)?;
        let signing_key = SigningKey::new(&Algorithm::SHA256, &server_secret);
        let server_finished_mac = self
            .key_schedule
            .calculate_finished_mac(&signing_key, &self.key_schedule.get_transcript())?;
        let finished: HandshakeMessage = Finished::new(server_finished_mac).into();
        // Send the Finished message.
        self.send_handshake_message(finished)?;

        // We can now *send* using the application traffic key,
        // but have to wait to receive the client Finished before receiving under that key.
        // We need to remember the handshake state from before the client Finished
        // in order to successfully verify the client Finished.
        let client_finished_transcript = self.key_schedule.get_transcript();
        self.key_schedule.finalize()?;
        let server_secret = self.key_schedule.get_server_application_secret()?;
        self.record_layer.set_send_key(&server_secret)?;
        let server_wait_finished: ServerWaitFinished<F> = self.into();
        Ok(server_wait_finished.initialize(client_secret, client_finished_transcript)?)
    }

    fn send_handshake_message(&mut self, msg: HandshakeMessage) -> Result<()> {
        let bytes = msg.to_bytes()?;
        self.key_schedule.add_to_transcript(&bytes);
        self.record_layer.send(RecordType::Handshake, &bytes)?;
        Ok(())
    }

    fn send_change_cipher_spec(&mut self) -> Result<()> {
        self.record_layer
            .send(RecordType::ChangeCipherSpec, &[0x01])?;
        self.record_layer.flush()?;
        Ok(())
    }
}

pub struct ServerWaitFinished<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
    client_secret: Vec<u8>,
    finished_transcript: Vec<u8>,
}

impl<F> From<ServerNegotiated<F>> for ServerWaitFinished<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn from(server_negotiated: ServerNegotiated<F>) -> Self {
        Self {
            key_schedule: server_negotiated.key_schedule,
            record_layer: server_negotiated.record_layer,
            client_secret: Vec::new(),
            finished_transcript: Vec::new(),
        }
    }
}

impl<F> ServerWaitFinished<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn initialize(mut self, client_secret: Vec<u8>, finished_transcript: Vec<u8>) -> Result<Self> {
        self.record_layer.flush()?;
        self.client_secret = client_secret;
        self.finished_transcript = finished_transcript;
        Ok(self)
    }

    fn recv_handshake_message(mut self, msg: HandshakeMessage) -> Result<ServerConnected<F>> {
        let signing_key = SigningKey::new(&Algorithm::SHA256, &self.client_secret);
        self.key_schedule.verify_finished_mac(
            &signing_key,
            &msg.get_verify_data(),
            &self.finished_transcript,
        )?;
        let client_secret = self.key_schedule.get_client_application_secret()?;
        self.record_layer.set_recv_key(&client_secret)?;
        let server_connected: ServerConnected<F> = self.into();
        Ok(server_connected.initialize()?)
    }
}

pub struct ServerConnected<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    key_schedule: KeySchedule,
    record_layer: RecordLayer<F>,
}

impl<F> From<ServerWaitFinished<F>> for ServerConnected<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn from(wait_finished: ServerWaitFinished<F>) -> Self {
        Self {
            key_schedule: wait_finished.key_schedule,
            record_layer: wait_finished.record_layer,
        }
    }
}

impl<F> ServerConnected<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    fn initialize(mut self) -> Result<Self> {
        self.record_layer.flush()?;
        Ok(self)
    }

    pub fn send_application_data(&mut self, data: &[u8]) -> Result<()> {
        self.record_layer.send(RecordType::ApplicationData, data)?;
        self.record_layer.flush()?;
        Ok(())
    }

    fn recv_application_data(&self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing_channel::utils;
    const PSK: &[u8] = b"aabbccddeeff";
    const PSK_ID: &[u8] = b"testkey";
    const CLIENT_HELLO: &str = "0100008e030330313031303130313031303130313031303130313031303130313031303130312030303030303030303030303030303030303030303030303030303030303030310002130101000043002b0003020304002d0002010000290032000d0007746573746b6579000000000021205f84ad32f7b6202f00377b0de82050feed09d13469537b33c62f7fe3bd8592cc";
    use bytes::Buf;
    #[test]
    fn test_server_start() {
        let mut sent_items = Vec::new();
        let mut server_start = ServerState::server_init(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            sent_items.push(data.to_vec());
            Ok(())
        })
        .unwrap();
        let msg = hex::decode(CLIENT_HELLO).unwrap();
        let mut buf = msg.clone().as_slice().to_bytes();
        let mut cpy = msg.as_slice().to_bytes();
        cpy.advance(1);
        let len = utils::read_u24(&mut cpy).unwrap();
        let bytes = utils::read_bytes(&mut buf, (len + 4) as usize).unwrap();
        match &mut server_start {
            ServerState::ServerStart(inner) => inner.key_schedule.add_to_transcript(&bytes),
            _ => panic!("Invalid type"),
        }
        let new_state = server_start.recv_handshake_message(&bytes).unwrap();
        if let ServerState::ServerWaitFinished(_) = new_state {
            // Okay
        } else {
            panic!("Invalid state")
        }
    }
}
