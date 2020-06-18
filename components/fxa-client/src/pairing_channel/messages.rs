/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::pairing_channel::extension::Extension;
use crate::pairing_channel::utils;
use crate::pairing_channel::utils::{
    HASH_LENGTH, PRE_SHARED_KEY_ID, SUPPORTED_VERSIONS_ID, TLS_AES_128_GCM_SHA256, VERSION_TLS_1_0,
    VERSION_TLS_1_2, VERSION_TLS_1_3,
};
use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::convert::TryInto;
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum HandshakeType {
    ClientHello = 1,
    ServerHello = 2,
    NewSessionTicket = 4,
    EncryptedExtensions = 8,
    Finished = 20,
    Unknown,
}

impl From<u8> for HandshakeType {
    fn from(num: u8) -> Self {
        match num {
            1 => HandshakeType::ClientHello,
            2 => HandshakeType::ServerHello,
            4 => HandshakeType::NewSessionTicket,
            8 => HandshakeType::EncryptedExtensions,
            20 => HandshakeType::Finished,
            _ => HandshakeType::Unknown,
        }
    }
}

impl Into<u8> for HandshakeType {
    fn into(self) -> u8 {
        match self {
            HandshakeType::ClientHello => 1,
            HandshakeType::ServerHello => 2,
            HandshakeType::NewSessionTicket => 4,
            HandshakeType::EncryptedExtensions => 8,
            HandshakeType::Finished => 20,
            HandshakeType::Unknown => 0,
        }
    }
}
// Base struct for generic reading/writing of handshake messages,
// which are all uniformly formatted as:
//
//  struct {
//    HandshakeType msg_type;    /* handshake type */
//    uint24 length;             /* bytes in message */
//    select(Handshake.msg_type) {
//        ... type specific cases here ...
//    };
//  } Handshake;
#[derive(Debug)]
pub struct HandshakeMessage {
    message_type: HandshakeType,
    message: Box<dyn Handshake>,
}

impl HandshakeMessage {
    pub fn from_bytes(buf: &[u8]) -> Result<HandshakeMessage> {
        // Each handshake message has a type and length prefix, per
        // https://tools.ietf.org/html/rfc8446#appendix-B.3
        let buf = buf.to_vec();
        let mut buf = buf.as_slice().to_bytes();
        let msg = Self::read(&mut buf)?;
        if buf.has_remaining() {
            anyhow::bail!("Error reading message");
        }
        Ok(msg)
    }

    pub fn get_extensions(&self) -> &Vec<Extension> {
        self.message.get_extensions()
    }

    pub fn get_type(&self) -> &HandshakeType {
        &self.message_type
    }

    pub fn get_session_id(&self) -> Vec<u8> {
        self.message.get_session_id()
    }

    pub fn get_verify_data(&self) -> Vec<u8> {
        self.message.get_verify_data()
    }

    fn read(buf: &mut Bytes) -> Result<HandshakeMessage> {
        let message_type = utils::read_u8(buf)?;
        let data_buf = utils::read_bytes_with_u24_len(buf)?;
        let mut data_buf = data_buf.as_slice().to_bytes();
        let ret = match message_type.into() {
            HandshakeType::ClientHello => HandshakeMessage {
                message_type: HandshakeType::ClientHello,
                message: Box::new(ClientHello::read(&mut data_buf)?),
            },
            HandshakeType::ServerHello => HandshakeMessage {
                message_type: HandshakeType::ServerHello,
                message: Box::new(ServerHello::read(&mut data_buf)?),
            },
            HandshakeType::NewSessionTicket => HandshakeMessage {
                message_type: HandshakeType::NewSessionTicket,
                message: Box::new(NewSessionTicket::read(&mut data_buf)?),
            },
            HandshakeType::EncryptedExtensions => HandshakeMessage {
                message_type: HandshakeType::EncryptedExtensions,
                message: Box::new(EncryptedExtensions::read(&mut data_buf)?),
            },
            HandshakeType::Finished => HandshakeMessage {
                message_type: HandshakeType::Finished,
                message: Box::new(Finished::read(&mut data_buf)?),
            },
            HandshakeType::Unknown => anyhow::bail!("Unknown handshake type"),
        };
        Ok(ret)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = BytesMut::new();
        self.write(&mut buf)?;
        Ok(buf.to_vec())
    }

    pub fn write(&self, buf: &mut BytesMut) -> Result<()> {
        buf.put_u8(self.message.get_type_tag());
        let mut data_buf = BytesMut::with_capacity(1024);
        self.message.write(&mut data_buf)?;
        let data_buf = data_buf.freeze();
        utils::write_u24(buf, data_buf.len().try_into()?);
        buf.put(data_buf);
        Ok(())
    }

    fn read_extensions(
        message_type: HandshakeType,
        buf: &mut Bytes,
    ) -> Result<(u16, Vec<Extension>)> {
        let mut res = Vec::new();
        let mut last_seen = 0;
        let data_buf = utils::read_bytes_with_u16_len(buf)?;
        let mut data_buf = data_buf.as_slice().to_bytes();
        while data_buf.has_remaining() {
            let ext = Extension::read(message_type, &mut data_buf)?;
            let seen_ext_fn = |seen_ext: &Extension| {
                seen_ext.get_type_tag() == ext.get_type_tag() && seen_ext.get_type_tag() != 0
            };
            if res.iter().any(seen_ext_fn) {
                anyhow::bail!("Extension already seen!")
            }
            last_seen = ext.get_type_tag();
            res.push(ext);
        }
        Ok((last_seen, res))
    }

    #[allow(dead_code)]
    fn write_extensions(&self, buf: &mut BytesMut, extensions: &[Extension]) -> Result<()> {
        let mut data_buf = BytesMut::new();
        extensions.iter().try_for_each(|extension| {
            extension.write(self.message.get_type_tag().into(), &mut data_buf)
        })?;
        buf.put_u16(data_buf.len().try_into()?);
        buf.put(data_buf);
        Ok(())
    }
}

pub trait Handshake: std::fmt::Debug {
    fn get_type_tag(&self) -> u8;
    fn write(&self, buf: &mut BytesMut) -> Result<()>;
    fn write_extensions(&self, buf: &mut BytesMut, extensions: &[Extension]) -> Result<()> {
        let mut data_buf = BytesMut::new();
        extensions
            .iter()
            .try_for_each(|extension| extension.write(self.get_type_tag().into(), &mut data_buf))?;
        buf.put_u16(data_buf.len().try_into()?);
        buf.put(data_buf);
        Ok(())
    }
    fn get_extensions(&self) -> &Vec<Extension>;
    fn get_session_id(&self) -> Vec<u8>;
    fn get_verify_data(&self) -> Vec<u8>;
}

// The ClientHello message:

// struct {
//   ProtocolVersion legacy_version = 0x0303;
//   Random random;
//   opaque legacy_session_id<0..32>;
//   CipherSuite cipher_suites<2..2^16-2>;
//   opaque legacy_compression_methods<1..2^8-1>;
//   Extension extensions<8..2^16-1>;
// } ClientHello;
#[derive(Debug)]
pub struct ClientHello {
    random: Vec<u8>,
    session_id: Vec<u8>,
    extensions: Vec<Extension>,
}

impl Handshake for ClientHello {
    fn get_verify_data(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_session_id(&self) -> Vec<u8> {
        self.session_id.clone()
    }
    fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn get_type_tag(&self) -> u8 {
        HandshakeType::ClientHello.into()
    }

    fn write(&self, buf: &mut BytesMut) -> Result<()> {
        buf.put_u16(VERSION_TLS_1_2);
        buf.put(self.random.as_slice());
        utils::write_bytes_with_u8_len(buf, &self.session_id)?;
        buf.put_u16(2);
        buf.put_u16(TLS_AES_128_GCM_SHA256);
        buf.put_u8(1);
        buf.put_u8(0);
        self.write_extensions(buf, &self.extensions)?;
        Ok(())
    }
}

impl From<ClientHello> for HandshakeMessage {
    fn from(client_hello: ClientHello) -> Self {
        Self {
            message_type: client_hello.get_type_tag().into(),
            message: Box::new(client_hello),
        }
    }
}

impl ClientHello {
    #[allow(dead_code)]
    pub fn new(random: Vec<u8>, session_id: Vec<u8>, extensions: Vec<Extension>) -> Self {
        Self {
            random,
            session_id,
            extensions,
        }
    }

    #[allow(dead_code)]
    pub fn get_session_id(&self) -> Vec<u8> {
        self.session_id.clone()
    }

    #[allow(dead_code)]
    pub fn get_extension(&self, ext_num: u16) -> Option<&Extension> {
        self.extensions
            .iter()
            .find(|ext| ext.get_type_tag() == ext_num)
    }

    fn read(buf: &mut Bytes) -> Result<Self> {
        // The legacy_version field may indicate an earlier version of TLS
        // for backwards compatibility, but must not predate TLS 1.0!
        let version = utils::read_u16(buf)?;
        if version < VERSION_TLS_1_0 {
            anyhow::bail!("Invalid protocol version")
        }
        let len = 32;
        let random = utils::read_bytes(buf, len as usize)?;
        let session_id = utils::read_bytes_with_u8_len(buf)?;
        let mut found = false;
        let inner_data = utils::read_bytes_with_u16_len(buf)?;
        let mut inner_data_buf = inner_data.as_slice().to_bytes();
        // We only support a single ciphersuite, but the peer may offer several.
        // Scan the list to confirm that the one we want is present.
        while inner_data_buf.has_remaining() {
            let cipher_suite = utils::read_u16(&mut inner_data_buf)?;
            if cipher_suite == TLS_AES_128_GCM_SHA256 {
                found = true;
            }
        }
        if !found {
            anyhow::bail!("Handshake failure")
        }
        // legacy_compression_methods must be a single zero byte for TLS1.3 ClientHellos.
        // It can be non-zero in previous versions of TLS, but we're not going to
        // make a successful handshake with such versions, so better to just bail out now.
        let legacy_compression_modes = utils::read_bytes_with_u8_len(buf)?;
        if legacy_compression_modes.len() != 1 || legacy_compression_modes[0] != 0x00 {
            anyhow::bail!("Illegal prameter for compression modes")
        }
        let (last_seen, extensions) =
            HandshakeMessage::read_extensions(HandshakeType::ClientHello, buf)?;

        if !extensions
            .iter()
            .find(|ext| ext.get_type_tag() == SUPPORTED_VERSIONS_ID)
            .ok_or_else(|| anyhow::Error::msg("Missing supported versions"))?
            .contains_version(VERSION_TLS_1_3)
        {
            anyhow::bail!("Does not support TLS1.3")
        }

        if extensions
            .iter()
            .any(|ext| ext.get_type_tag() == PRE_SHARED_KEY_ID)
            && last_seen != PRE_SHARED_KEY_ID
        {
            anyhow::bail!("Last seen was not the pre shared key extension!")
        }
        Ok(Self {
            random,
            session_id,
            extensions,
        })
    }
}

// The ServerHello message:
//
//  struct {
//      ProtocolVersion legacy_version = 0x0303;    /* TLS v1.2 */
//      Random random;
//      opaque legacy_session_id_echo<0..32>;
//      CipherSuite cipher_suite;
//      uint8 legacy_compression_method = 0;
//      Extension extensions < 6..2 ^ 16 - 1 >;
//  } ServerHello;
#[derive(Debug)]
pub struct ServerHello {
    random: Vec<u8>,
    session_id: Vec<u8>,
    extensions: Vec<Extension>,
}

impl From<ServerHello> for HandshakeMessage {
    fn from(server_hello: ServerHello) -> Self {
        Self {
            message_type: server_hello.get_type_tag().into(),
            message: Box::new(server_hello),
        }
    }
}

impl Handshake for ServerHello {
    fn get_verify_data(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_session_id(&self) -> Vec<u8> {
        self.session_id.clone()
    }
    fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn get_type_tag(&self) -> u8 {
        HandshakeType::ServerHello.into()
    }

    fn write(&self, buf: &mut BytesMut) -> Result<()> {
        buf.put_u16(VERSION_TLS_1_2);
        buf.put(self.random.as_slice());
        utils::write_bytes_with_u8_len(buf, &self.session_id)?;
        buf.put_u16(TLS_AES_128_GCM_SHA256);
        buf.put_u8(0);
        self.write_extensions(buf, &self.extensions)
    }
}

impl ServerHello {
    pub fn new(random: Vec<u8>, session_id: Vec<u8>, extensions: Vec<Extension>) -> Self {
        Self {
            random,
            session_id,
            extensions,
        }
    }

    #[allow(dead_code)]
    pub fn get_session_id(&self) -> Vec<u8> {
        self.session_id.clone()
    }

    #[allow(dead_code)]
    pub fn get_extension(&self, ext_num: u16) -> Option<&Extension> {
        self.extensions
            .iter()
            .find(|ext| ext.get_type_tag() == ext_num)
    }
    fn read(buf: &mut Bytes) -> Result<Self> {
        let version = utils::read_u16(buf)?;
        if version != VERSION_TLS_1_2 {
            anyhow::bail!("Invalid protocol version")
        }
        let len = 32;
        let random = utils::read_bytes(buf, len as usize)?;
        let session_id = utils::read_bytes_with_u8_len(buf)?;
        if utils::read_u16(buf)? != TLS_AES_128_GCM_SHA256 {
            anyhow::bail!("Illegal number for ciphersuite")
        }
        if utils::read_u8(buf)? != 0 {
            anyhow::bail!("Illegal number for legacy compression modes");
        }

        let (_, extensions) = HandshakeMessage::read_extensions(HandshakeType::ServerHello, buf)?;

        if !extensions
            .iter()
            .find(|ext| ext.get_type_tag() == SUPPORTED_VERSIONS_ID)
            .ok_or_else(|| anyhow::Error::msg("Missing supported versions"))?
            .is_selected_version(VERSION_TLS_1_3)
        {
            anyhow::bail!("Does not support TLS1.3")
        }
        Ok(Self {
            random,
            session_id,
            extensions,
        })
    }
}

// The NewSessionTicket message:
//
//   struct {
//    uint32 ticket_lifetime;
//    uint32 ticket_age_add;
//    opaque ticket_nonce < 0..255 >;
//    opaque ticket < 1..2 ^ 16 - 1 >;
//    Extension extensions < 0..2 ^ 16 - 2 >;
//  } NewSessionTicket;
//
// We don't actually make use of these, but we need to be able
// to accept them and do basic validation.
#[derive(Debug)]
pub struct NewSessionTicket {
    ticket_lifetime: u32,
    ticket_age_add: u32,
    ticket_nonce: Vec<u8>,
    ticket: Vec<u8>,
    extensions: Vec<Extension>,
}

impl From<NewSessionTicket> for HandshakeMessage {
    fn from(new_session_ticket: NewSessionTicket) -> Self {
        Self {
            message_type: new_session_ticket.get_type_tag().into(),
            message: Box::new(new_session_ticket),
        }
    }
}

impl Handshake for NewSessionTicket {
    fn get_verify_data(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_session_id(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn get_type_tag(&self) -> u8 {
        HandshakeType::NewSessionTicket.into()
    }

    fn write(&self, buf: &mut BytesMut) -> Result<()> {
        buf.put_u32(self.ticket_lifetime);
        buf.put_u32(self.ticket_age_add);
        utils::write_bytes_with_u8_len(buf, &self.ticket_nonce)?;
        utils::write_bytes_with_u16_len(buf, &self.ticket)?;
        self.write_extensions(buf, &self.extensions)
    }
}

impl NewSessionTicket {
    fn read(buf: &mut Bytes) -> Result<Self> {
        let ticket_lifetime = utils::read_u32(buf)?;
        let ticket_age_add = utils::read_u32(buf)?;
        let ticket_nonce = utils::read_bytes_with_u8_len(buf)?;
        let ticket = utils::read_bytes_with_u16_len(buf)?;
        if ticket.is_empty() {
            anyhow::bail!("Invalid ticket length");
        }
        let (_, extensions) =
            HandshakeMessage::read_extensions(HandshakeType::NewSessionTicket, buf)?;
        Ok(Self {
            ticket_lifetime,
            ticket_age_add,
            ticket_nonce,
            ticket,
            extensions,
        })
    }
}
// The EncryptedExtensions message:
//
//  struct {
//    Extension extensions < 0..2 ^ 16 - 1 >;
//  } EncryptedExtensions;
//
// We don't actually send any EncryptedExtensions,
// but still have to send an empty message.
#[derive(Debug)]
pub struct EncryptedExtensions {
    extensions: Vec<Extension>,
}

impl From<EncryptedExtensions> for HandshakeMessage {
    fn from(encrypted_ext: EncryptedExtensions) -> Self {
        Self {
            message_type: encrypted_ext.get_type_tag().into(),
            message: Box::new(encrypted_ext),
        }
    }
}

impl Handshake for EncryptedExtensions {
    fn get_verify_data(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_session_id(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn get_type_tag(&self) -> u8 {
        HandshakeType::EncryptedExtensions.into()
    }

    fn write(&self, buf: &mut BytesMut) -> Result<()> {
        self.write_extensions(buf, &self.extensions)
    }
}

impl EncryptedExtensions {
    pub fn new(extensions: Vec<Extension>) -> Self {
        Self { extensions }
    }

    #[allow(dead_code)]
    pub fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn read(buf: &mut Bytes) -> Result<Self> {
        let (_, extensions) =
            HandshakeMessage::read_extensions(HandshakeType::EncryptedExtensions, buf)?;
        Ok(Self { extensions })
    }
}

// The Finished message:
//
// struct {
//   opaque verify_data[Hash.length];
// } Finished;
#[derive(Debug)]
pub struct Finished {
    verify_data: Vec<u8>,
    extensions: Vec<Extension>,
}

impl From<Finished> for HandshakeMessage {
    fn from(finished: Finished) -> Self {
        Self {
            message_type: finished.get_type_tag().into(),
            message: Box::new(finished),
        }
    }
}

impl Handshake for Finished {
    fn get_verify_data(&self) -> Vec<u8> {
        self.verify_data.clone()
    }
    fn get_session_id(&self) -> Vec<u8> {
        vec![0u8; 0]
    }
    fn get_extensions(&self) -> &Vec<Extension> {
        &self.extensions
    }
    fn get_type_tag(&self) -> u8 {
        HandshakeType::Finished.into()
    }

    fn write(&self, buf: &mut BytesMut) -> Result<()> {
        buf.put(self.verify_data.as_slice());
        Ok(())
    }
}

impl Finished {
    pub fn new(verify_data: Vec<u8>) -> Self {
        Self {
            extensions: Vec::new(),
            verify_data,
        }
    }
    fn read(buf: &mut Bytes) -> Result<Self> {
        let verify_data = utils::read_bytes(buf, HASH_LENGTH)?;
        Ok(Self {
            extensions: Vec::new(),
            verify_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CLIENT_HELLO: &str = "0100008e030330313031303130313031303130313031303130313031303130313031303130312030303030303030303030303030303030303030303030303030303030303030310002130101000043002b0003020304002d0002010000290032000d0007746573746b6579000000000021205f84ad32f7b6202f00377b0de82050feed09d13469537b33c62f7fe3bd8592cc";
    const SERVER_HELLO: &str = "0200005403033032303230323032303230323032303230323032303230323032303230323032203030303030303030303030303030303030303030303030303030303030303031130100000c002b00020304002900020000";
    const CLIENT_HELLO_BIG: &str = "010002c0030330313031303130313031303130313031303130313031303130313031303130312030303030303030303030303030303030303030303030303030303030303030310034130213011303cca8c030c02fc028c027c014c013c012ccaa009f009e006b0067003900330016009d009c003d003c0035002f000a010002430016000000170000000b00020100000a00160014001d001e00180017001901000101010201030104000d001800160806080b0805080a0804080906010501040103010201002b000504030403050033006b00690017004104281ccb4d2bc57cf3bd922632101bbe3f16e99cb8e22e60b972fc9102ff03feada6a8fc82982f9c3c92ab982d5253d7e03c0ef6fec89c71854b1d620d4f895f1b001d0020a1d303ffb674d592128899513a0fb1f2a43ec477772ff94e860536b38a59331f002d0003020001000f000101001c00024001001500b1000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002900bc0035000b612064756d6d79206b6579000000000007746573746b6579000000000011616e6f746865722064756d6d79206b657900000000008330c6b42489148aab36e2649d1e8c9c017aedf5882061812caaf13680210120a101d823dff9cd8c17210f1cbfff99fc0b9b201d0d160f28139f00cb54295153ab9c56b233e5c609efc4e3faa9e6ecafde91443081bdcd874e98150d5ef5d719441f508b7e0088c3c09693d090a33ec6938264837151ab85f953355434dad4bc78e9fa7f";

    #[test]
    fn test_client_hello_read() {
        let msg = hex::decode(CLIENT_HELLO).unwrap();
        let mut buf: Bytes = msg.as_slice().to_bytes();
        let hm = HandshakeMessage::read(&mut buf).unwrap();
        assert_eq!(*hm.get_type(), HandshakeType::ClientHello);
        // ADD MORE TESTS FOR SESSION_ID, RANDOM, EXTENSIONS ETC!!!!
        // But they seem okay from my "dig into the debugger" attempt
        assert_eq!(hex::encode(hm.get_session_id()), "");
    }

    #[test]
    fn test_server_hello_read() {
        let msg = hex::decode(SERVER_HELLO).unwrap();
        let mut buf: Bytes = msg.as_slice().to_bytes();
        let hm = HandshakeMessage::read(&mut buf).unwrap();
        assert_eq!(*hm.get_type(), HandshakeType::ServerHello);
    }

    #[test]
    fn test_client_hello_write() {
        let msg = hex::decode(CLIENT_HELLO).unwrap();
        let mut buf: Bytes = msg.as_slice().to_bytes();
        let hm = HandshakeMessage::read(&mut buf).unwrap();
        assert_eq!(*hm.get_type(), HandshakeType::ClientHello);
        let mut out_buf = BytesMut::with_capacity(1024);
        hm.write(&mut out_buf).unwrap();
        let other = out_buf.freeze();
        assert_eq!(hex::encode(&other), CLIENT_HELLO);
    }

    #[test]
    fn test_server_hello_write() {
        let msg = hex::decode(SERVER_HELLO).unwrap();
        let mut buf: Bytes = msg.as_slice().to_bytes();
        let hm = HandshakeMessage::read(&mut buf).unwrap();
        assert_eq!(*hm.get_type(), HandshakeType::ServerHello);
        let mut out_buf = BytesMut::with_capacity(1024);
        hm.write(&mut out_buf).unwrap();
        let other = out_buf.freeze();
        assert_eq!(hex::encode(&other), SERVER_HELLO);
    }

    #[test]
    fn test_big_client_hello() {
        let msg = hex::decode(CLIENT_HELLO_BIG).unwrap();
        let mut buf: Bytes = msg.as_slice().to_bytes();
        let hm = HandshakeMessage::read(&mut buf).unwrap();
        assert_eq!(*hm.get_type(), HandshakeType::ClientHello);
        // Not gonna test the writing, because the message has a
        // Bunch of extensions we do not support (:
    }
}
