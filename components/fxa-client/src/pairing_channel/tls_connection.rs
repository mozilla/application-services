/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;

// Adding this because although a TLS implementation
// exists in this module, if a different approved
// implementation (whether internal or exteral) can support the required PSK and implements
// those two functions, then it should be usable.
pub trait Connection {
    fn send(&mut self, data: &[u8]) -> Result<()>;
    fn recv(&mut self, data: &[u8]) -> Result<Vec<u8>>;
}

use super::client_states::ClientState;
use super::record_layer::RecordType;
use super::server_states::ServerState;
use super::utils;
use bytes::Buf;
use std::cell::RefCell;
pub struct ServerConnection<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    state: RefCell<ServerState<F>>,
}

#[allow(dead_code)]
pub struct ClientConnection<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    state: RefCell<ClientState<F>>,
}

impl<F> ServerConnection<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    pub fn new(psk: Vec<u8>, psk_id: Vec<u8>, callback: F) -> Result<Self> {
        let state = ServerState::server_init(psk, psk_id, callback)?;
        Ok(Self {
            state: RefCell::new(state),
        })
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> Connection for ServerConnection<F> {
    fn send(&mut self, data: &[u8]) -> Result<()> {
        match self.state.get_mut() {
            ServerState::ServerConnected(inner) => inner.send_application_data(data),
            _ => anyhow::bail!("Invalid state"),
        }
    }

    fn recv(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        // Decrypt the data using the record layer.
        // We expect to receive precisely one record at a time.
        let (recored_type, bytes) = self.state.borrow_mut().recv(data)?;
        match recored_type {
            RecordType::ApplicationData => self.state.borrow_mut().recv_application_data(&bytes),
            RecordType::Handshake => {
                let mut handshake_rev_buff = bytes.as_slice().to_bytes();
                // Multiple handshake messages may be coalesced into a single record.
                if !handshake_rev_buff.has_remaining() {
                    anyhow::bail!("Buffer too small")
                }
                while handshake_rev_buff.has_remaining() {
                    // Each handshake messages has a type and length prefix, per
                    // https://tools.ietf.org/html/rfc8446#appendix-B.3
                    let mut other = handshake_rev_buff.clone();
                    other.advance(1);
                    let mlen = utils::read_u24(&mut other)?;
                    let message = utils::read_bytes(&mut handshake_rev_buff, mlen as usize + 4)?;
                    self.state.borrow_mut().add_to_transcript(&message)?;
                    let old_val = self.state.replace(ServerState::Invalid);
                    self.state
                        .replace(old_val.recv_handshake_message(&message)?);
                }
                Ok(vec![0u8; 0])
            }
            _ => anyhow::bail!("Invalid record!"),
        }
    }
}

impl<F> ClientConnection<F>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    #[allow(dead_code)]
    pub fn new(psk: Vec<u8>, psk_id: Vec<u8>, callback: F) -> Result<Self> {
        let state = ClientState::client_init(psk, psk_id, callback)?;
        Ok(Self {
            state: RefCell::new(state),
        })
    }
}

impl<F: FnMut(&[u8]) -> Result<()>> Connection for ClientConnection<F> {
    #[allow(dead_code)]
    fn send(&mut self, data: &[u8]) -> Result<()> {
        match self.state.get_mut() {
            ClientState::ClientConnected(inner) => inner.send_application_data(data),
            _ => anyhow::bail!("Invalid state"),
        }
    }

    #[allow(dead_code)]
    fn recv(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let (recored_type, bytes) = self.state.borrow_mut().recv(data)?;
        match recored_type {
            RecordType::ApplicationData => self.state.borrow_mut().recv_application_data(&bytes),
            RecordType::ChangeCipherSpec => {
                self.state.borrow_mut().recv_change_cipher_spec(&bytes)?;
                Ok(vec![0u8; 0])
            }
            RecordType::Handshake => {
                let mut handshake_rev_buff = bytes.as_slice().to_bytes();
                if !handshake_rev_buff.has_remaining() {
                    anyhow::bail!("Buffer too small")
                }
                while handshake_rev_buff.has_remaining() {
                    let mut other = handshake_rev_buff.clone();
                    other.advance(1);
                    let mlen = utils::read_u24(&mut other)?;
                    let message = utils::read_bytes(&mut handshake_rev_buff, mlen as usize + 4)?;
                    self.state.borrow_mut().add_to_transcript(&message)?;
                    let old_val = self.state.replace(ClientState::Invalid);
                    self.state
                        .replace(old_val.recv_handshake_message(&message)?);
                }
                Ok(vec![0u8; 0])
            }
            _ => anyhow::bail!("Invalid record!"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PSK: &[u8] = b"aabbccddeeff";
    const PSK_ID: &[u8] = b"testkey";
    use std::cell::RefCell;
    #[test]
    fn test_server_client_handshake() {
        let server_sent_buf = RefCell::new(Vec::new());
        let client_sent_buf = RefCell::new(Vec::new());
        let mut server = ServerConnection::new(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            server_sent_buf.borrow_mut().push(data.to_vec());
            Ok(())
        })
        .unwrap();
        let mut client = ClientConnection::new(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            client_sent_buf.borrow_mut().push(data.to_vec());
            Ok(())
        })
        .unwrap();
        server.recv(&(client_sent_buf.borrow()[0])).unwrap();
        if let ServerState::ServerWaitFinished(_) = server.state.get_mut() {
            // Okay
        } else {
            panic!("Invalid state")
        }
        client.recv(&server_sent_buf.borrow()[0]).unwrap();
        client.recv(&server_sent_buf.borrow()[1]).unwrap();
        client.recv(&server_sent_buf.borrow()[2]).unwrap();
        server.recv(&client_sent_buf.borrow()[1]).unwrap();
        if let ServerState::ServerConnected(_) = server.state.get_mut() {
            // Okay
        } else {
            panic!("Should be connected!");
        }
        if let ClientState::ClientConnected(_) = client.state.get_mut() {
            // Okay
        } else {
            panic!("Should be connected!")
        }
    }

    #[test]
    fn test_send_application_data() {
        let server_sent_buf = RefCell::new(Vec::new());
        let client_sent_buf = RefCell::new(Vec::new());
        let mut server = ServerConnection::new(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            server_sent_buf.borrow_mut().push(data.to_vec());
            Ok(())
        })
        .unwrap();
        let mut client = ClientConnection::new(PSK.to_vec(), PSK_ID.to_vec(), |data| {
            client_sent_buf.borrow_mut().push(data.to_vec());
            Ok(())
        })
        .unwrap();
        server.recv(&(client_sent_buf.borrow()[0])).unwrap();
        if let ServerState::ServerWaitFinished(_) = server.state.get_mut() {
            // Okay
        } else {
            panic!("Invalid state")
        }
        client.recv(&server_sent_buf.borrow()[0]).unwrap();
        client.recv(&server_sent_buf.borrow()[1]).unwrap();
        client.recv(&server_sent_buf.borrow()[2]).unwrap();
        server.recv(&client_sent_buf.borrow()[1]).unwrap();
        if let ServerState::ServerConnected(_) = server.state.get_mut() {
            // Okay
        } else {
            panic!("Should be connected!");
        }
        if let ClientState::ClientConnected(_) = client.state.get_mut() {
            // Okay
        } else {
            panic!("Should be connected!")
        }
        server.send(b"hello client!").unwrap();
        let msg = client.recv(&server_sent_buf.borrow()[3]).unwrap();
        assert_eq!(std::str::from_utf8(&msg).unwrap(), "hello client!");
    }
}
