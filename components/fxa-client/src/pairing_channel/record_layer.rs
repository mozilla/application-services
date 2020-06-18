/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use rc_crypto::{
    aead,
    aead::{OpeningKey, SealingKey, AES_128_GCM},
    digest, hkdf, hmac,
};
use std::convert::TryFrom;
use std::convert::TryInto;

// Encrypting at most 2^24 records will force us to stay
// below data limits on AES-GCM encryption key use,
const MAX_SEQUENCE_NUMBER: u32 = 16_777_216; // 2^24
const MAX_RECORD_SIZE: u16 = 16_384; // 2^14
const RECORD_HEADER_SIZE: usize = 5;
const AEAD_SIZE_INFLATION: usize = 16;
const VERSION_TLS_1_2: u16 = 0x0303;
const VERSION_TLS_1_0: u16 = 0x0301;
const MAX_ENCRYPTED_RECORD_SIZE: u16 = MAX_RECORD_SIZE + 256;

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum RecordType {
    ChangeCipherSpec = 20,
    Alert = 21,
    Handshake = 22,
    ApplicationData = 23,
}

impl TryFrom<u8> for RecordType {
    type Error = anyhow::Error;
    fn try_from(num: u8) -> Result<RecordType> {
        Ok(match num {
            20 => RecordType::ChangeCipherSpec,
            21 => RecordType::Alert,
            22 => RecordType::Handshake,
            23 => RecordType::ApplicationData,
            _ => anyhow::bail!("Invalid record type"),
        })
    }
}

impl Into<u8> for RecordType {
    fn into(self) -> u8 {
        match self {
            RecordType::ChangeCipherSpec => 20,
            RecordType::Alert => 21,
            RecordType::Handshake => 22,
            RecordType::ApplicationData => 23,
        }
    }
}

pub struct Encryptor {
    key: SealingKey,
    iv: Vec<u8>,
    seq_num: u32,
}

impl Encryptor {
    fn new(key: &[u8]) -> Result<Self> {
        let signing_key = hmac::SigningKey::new(&digest::Algorithm::SHA256, key);
        let key = hkdf::expand_label(&signing_key, "key", b"", 16)?;
        let key = SealingKey::new(&AES_128_GCM, &key)?;
        let empty = vec![0u8; 0];
        let iv = hkdf::expand_label(&signing_key, "iv", &empty, 12)?;
        Ok(Self {
            key,
            iv,
            seq_num: 0,
        })
    }

    fn nonce(&mut self) -> Result<Vec<u8>> {
        let nonce = get_nonce(&self.iv, self.seq_num)?;
        self.seq_num += 1;
        if self.seq_num > MAX_SEQUENCE_NUMBER {
            anyhow::bail!("Internal error: Sequence number too high");
        }
        Ok(nonce)
    }

    fn encrypt(&mut self, plaintext: &[u8], additional_data: &[u8]) -> Result<Vec<u8>> {
        let additional_data = aead::Aad::from(additional_data);
        let nonce = aead::Nonce::try_assume_unique_for_key(&AES_128_GCM, &self.nonce()?)?;
        let encrypted = aead::seal(&self.key, nonce, additional_data, plaintext)?;
        Ok(encrypted)
    }
}

pub struct Decryptor {
    key: OpeningKey,
    iv: Vec<u8>,
    seq_num: u32,
}

impl Decryptor {
    fn new(key: &[u8]) -> Result<Self> {
        let signing_key = hmac::SigningKey::new(&digest::Algorithm::SHA256, key);
        let key = hkdf::expand_label(&signing_key, "key", b"", 16)?;
        let key = OpeningKey::new(&AES_128_GCM, &key)?;
        let iv = hkdf::expand_label(&signing_key, "iv", b"", 12)?;
        Ok(Self {
            key,
            iv,
            seq_num: 0,
        })
    }

    fn nonce(&mut self) -> Result<Vec<u8>> {
        let nonce = get_nonce(&self.iv, self.seq_num)?;
        self.seq_num += 1;
        if self.seq_num > MAX_SEQUENCE_NUMBER {
            anyhow::bail!("Internal error: Sequence number too high");
        }
        Ok(nonce)
    }

    fn decrypt(&mut self, ciphertext: &[u8], additional_data: &[u8]) -> Result<Vec<u8>> {
        let additional_data = aead::Aad::from(additional_data);
        let nonce = aead::Nonce::try_assume_unique_for_key(&AES_128_GCM, &self.nonce()?)?;
        let decrypted = aead::open(&self.key, nonce, additional_data, ciphertext)?;
        Ok(decrypted)
    }
}

// Ref https://tools.ietf.org/html/rfc8446#section-5.3:
// * left-pad the sequence number with zeros to IV_LENGTH
// * xor with the provided iv
// Our sequence numbers are always less than 2^24, so fit in a Uint32
// in the last 4 bytes of the nonce.
fn get_nonce(iv: &[u8], seq_num: u32) -> Result<Vec<u8>> {
    let nonce = iv;
    let last_bytes = nonce
        .get(nonce.len() - 4..)
        .ok_or_else(|| anyhow::Error::msg("Invalid iv"))?;
    let last_bytes: [u8; 4] = last_bytes.try_into()?;
    let last_bytes = u32::from_be_bytes(last_bytes);
    let to_replace_with = last_bytes ^ seq_num;
    let mut nonce = nonce
        .get(..nonce.len() - 4)
        .ok_or_else(|| anyhow::Error::msg("Invalid nonce"))?
        .to_vec();
    nonce.extend_from_slice(&to_replace_with.to_be_bytes());
    Ok(nonce)
}

///
/// This implements the "record layer" for TLS1.3, as defined in
/// https://tools.ietf.org/html/rfc8446#section-5.
///
/// The record layer is responsible for encrypting/decrypting bytes to be
/// sent over the wire, including stateful management of sequence numbers
/// for the incoming and outgoing stream.
///
/// The main interface is the RecordLayer struct, which takes a callback function
/// sending data and can be used like so:
///
///    let rl = RecordLayer::new(|data| {
///      // application-specific sending logic here.
///    });
///
///    // Records are sent and received in plaintext by default,
///    // until you specify the key to use.
///    rl.set_send_key(key)
///
///    // Send some data by specifying the record type and the bytes.
///    // Where allowed by the record type, it will be buffered until
///    // explicitly flushed, and then sent by calling the callback.
///    rl.send(RecordType::Handshake, <bytes for a handshake message>)?;
///    rl.send(RecordType::handshake, <bytes for another handshake message>)?;
///    rl.flush()?;
///
///    // Separate keys are used for sending and receiving.
///    rl.set_recv_key(key);
///
///    // When data is received, push it into the RecordLayer
///    // [type, bytes] will be returned
///    // pair for each message parsed from the data.
///    let (type, bytes) = rl.recv(data_received_from_peer)?;
pub struct RecordLayer<F: FnMut(&[u8]) -> Result<()>> {
    send_callback: F,
    send_encryption_state: Option<Encryptor>,
    // Add send_error
    recv_decryption_state: Option<Decryptor>,
    pending_state: Option<(RecordType, Vec<u8>)>,
}

impl<F: FnMut(&[u8]) -> Result<()>> RecordLayer<F> {
    pub fn new(send_callback: F) -> Self {
        Self {
            send_callback,
            send_encryption_state: None,
            recv_decryption_state: None,
            pending_state: None,
        }
    }

    pub fn set_send_key(&mut self, key: &[u8]) -> Result<()> {
        self.flush()?;
        self.send_encryption_state = Some(Encryptor::new(key)?);
        Ok(())
    }

    pub fn set_recv_key(&mut self, key: &[u8]) -> Result<()> {
        self.recv_decryption_state = Some(Decryptor::new(key)?);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_send_error(_: &str) -> Result<()> {
        unimplemented!();
    }

    #[allow(dead_code)]
    pub fn set_recv_error(_: &str) -> Result<()> {
        unimplemented!();
    }

    pub fn send(&mut self, record_type: RecordType, data: &[u8]) -> Result<()> {
        // TODO: Check for send_err

        // Forbid sending data that doesn't fit into a single record.
        // We do not support fragmentation over multiple records.
        let len: u16 = data.len().try_into()?;
        if len > MAX_RECORD_SIZE {
            anyhow::bail!("Cannot fit data in record");
        }

        // Flush if we're switching to a different record type.
        if let Some((pending_type, pending_buf)) = &self.pending_state {
            if record_type != *pending_type
                || (pending_buf.len() + data.len()) as u16 > MAX_RECORD_SIZE
            {
                self.flush()?;
            }
        }

        // Start a new pending record if necessary.
        // We reserve space at the start of the buffer for the record header,
        // which is conveniently always a fixed size.
        if self.pending_state.is_none() {
            let mut buf = Vec::new();
            let zeros_header = vec![0u8; RECORD_HEADER_SIZE];
            buf.extend_from_slice(&zeros_header);
            self.pending_state = Some((record_type, buf));
        }
        if let Some((_, record_buf)) = &mut self.pending_state {
            record_buf.extend_from_slice(data);
        } else {
            anyhow::bail!("Somehow the buffer is not there! Impossible!")
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if let Some((record_type, buf)) = &mut self.pending_state {
            // Add send_error checking here!!!
            let mut record_type: u8 = (*record_type).into();
            let mut buf = buf.clone();

            // If we're encrypting, turn the existing buffer contents into a `TLSInnerPlaintext` by
            // appending the type. We don't do any zero-padding, although the spec allows it.
            let mut inflation = 0;
            let mut inner_plain_text = None;
            if self.send_encryption_state.is_some() {
                buf.extend_from_slice(&record_type.to_be_bytes());
                inner_plain_text = Some(buf[RECORD_HEADER_SIZE..].to_vec());
                inflation = AEAD_SIZE_INFLATION;
                record_type = RecordType::ApplicationData.into();
            }

            // Write the common header for either `TLSPlaintext` or `TLSCiphertext` record.
            let len = buf.len() - RECORD_HEADER_SIZE + inflation;
            let record_type = (record_type as u8).to_be_bytes();
            let version = VERSION_TLS_1_2.to_be_bytes();
            let length = (len as u16).to_be_bytes();
            let buf_iter = record_type
                .iter()
                .chain(version.iter())
                .chain(length.iter());

            // Followed by different payload depending on encryption status.
            let data = if let Some(encyption_state) = &mut self.send_encryption_state {
                let additional_data: Vec<u8> = buf_iter.clone().cloned().collect();
                encyption_state.encrypt(&inner_plain_text.unwrap(), &additional_data)?
            } else {
                buf[RECORD_HEADER_SIZE..].to_vec()
            };
            buf = buf_iter.chain(data.iter()).cloned().collect();
            self.pending_state = None;
            (self.send_callback)(&buf)
        } else {
            // If there's nothing to flush, bail out early.
            Ok(())
        }
    }

    pub fn recv(&mut self, data: &[u8]) -> Result<(RecordType, Vec<u8>)> {
        // For simplicity, we assume that the given data contains exactly one record.
        // Peers using this library will send one record at a time over the websocket
        // connection, and we can assume that the server-side websocket bridge will split
        // up any traffic into individual records if we ever start interoperating with
        // peers using a different TLS implementation.
        // Similarly, we assume that handshake messages will not be fragmented across
        // multiple records. This should be trivially true for the PSK-only mode used
        // by this library, but we may want to relax it in future for interoperability
        // with e.g. large ClientHello messages that contain lots of different options.

        // The data to read is either a TLSPlaintext or TLSCiphertext struct,
        // depending on whether record protection has been enabled yet:
        //
        //    struct {
        //        ContentType type;
        //        ProtocolVersion legacy_record_version;
        //        uint16 length;
        //        opaque fragment[TLSPlaintext.length];
        //    } TLSPlaintext;
        //
        //    struct {
        //        ContentType opaque_type = application_data; /* 23 */
        //        ProtocolVersion legacy_record_version = 0x0303; /* TLS v1.2 */
        //        uint16 length;
        //        opaque encrypted_record[TLSCiphertext.length];
        //    } TLSCiphertext;
        //
        let mut pos = 0;
        let record_type: RecordType = data[pos].try_into()?;
        pos += 1;
        // The spec says legacy_record_version "MUST be ignored for all purposes",
        // but we know TLS1.3 implementations will only ever emit two possible values,
        // so it seems useful to bail out early if we receive anything else.
        let version = read_u16(data, pos)?;
        pos += 2;
        if version != VERSION_TLS_1_2
            && (self.recv_decryption_state.is_some() || version != VERSION_TLS_1_0)
        {
            anyhow::bail!("Internal error: Version is not supported")
        }
        let length = read_u16(data, pos)?;
        pos += 2;
        if self.recv_decryption_state.is_none() || record_type == RecordType::ChangeCipherSpec {
            self.read_plaintext_record(record_type, length, data, pos)
        } else {
            self.read_encrypted_record(record_type, length, data, pos)
        }
    }

    fn read_plaintext_record(
        &mut self,
        record_type: RecordType,
        length: u16,
        data: &[u8],
        pos: usize,
    ) -> Result<(RecordType, Vec<u8>)> {
        if length > MAX_RECORD_SIZE as u16 || data.len() - pos < length as usize {
            anyhow::bail!("Length is greater than a record!");
        }
        if data.len() > length as usize + RECORD_HEADER_SIZE {
            anyhow::bail!("Extra bytes after record");
        }
        Ok((record_type, (&data[pos..pos + length as usize]).to_vec()))
    }

    fn read_encrypted_record(
        &mut self,
        record_type: RecordType,
        length: u16,
        data: &[u8],
        pos: usize,
    ) -> Result<(RecordType, Vec<u8>)> {
        if length > MAX_ENCRYPTED_RECORD_SIZE || data.len() - pos < length as usize {
            anyhow::bail!("Encrypted record too large!");
        }
        if record_type != RecordType::ApplicationData {
            anyhow::bail!("Decode error: Type should be application data");
        }
        if data.len() > length as usize + RECORD_HEADER_SIZE {
            anyhow::bail!("Extra bytes after record");
        }
        // Decrypt and decode the contained `TLSInnerPlaintext` struct:
        //
        //    struct {
        //        opaque content[TLSPlaintext.length];
        //        ContentType type;
        //        uint8 zeros[length_of_padding];
        //    } TLSInnerPlaintext;
        //
        // The additional data for the decryption is the `TLSCiphertext` record
        // header, which is a fixed size and immediately prior to current buffer position.
        let mut pos = pos - RECORD_HEADER_SIZE;
        let additional_data = &data[pos..pos + RECORD_HEADER_SIZE];
        pos += RECORD_HEADER_SIZE;
        let ciphertext = &data[pos..pos + length as usize];
        if let Some(decrypt_state) = &mut self.recv_decryption_state {
            let padded_plain_text = decrypt_state.decrypt(ciphertext, additional_data)?;
            let (i, record_type) = padded_plain_text
                .iter()
                .enumerate()
                .rev()
                .find(|(_, &val)| val != 0)
                .ok_or_else(|| anyhow::Error::msg("No type found!"))?;
            let record_type: RecordType = (*record_type).try_into()?;
            if record_type == RecordType::ChangeCipherSpec {
                anyhow::bail!("Change Cipher Spec must be in plaintext!");
            }
            return Ok((record_type, (&padded_plain_text[..i]).to_vec()));
        }
        anyhow::bail!("Decrypt state does not exist!")
    }
}

fn read_u16(data: &[u8], pos: usize) -> Result<u16> {
    let res: [u8; 2] = (&data[pos..pos + 2]).try_into()?;
    Ok(u16::from_be_bytes(res))
}

#[cfg(test)]
mod tests {
    use super::*;
    const SERVER_RAW_APP_DATA: &[u8] = b"hello world";
    use rc_crypto::rand;
    use std::cell::RefCell;

    impl Decryptor {
        fn decrypt_inner_plain_text(&mut self, data: &[u8]) -> (Vec<u8>, RecordType) {
            let plain_text = self.decrypt(&data[5..], &data[..5]).unwrap();
            (
                (&plain_text[..plain_text.len() - 1]).to_vec(),
                (*plain_text.last().unwrap()).try_into().unwrap(),
            )
        }
    }

    struct EncryptedInnerArgs {
        outer_trailer: Vec<u8>,
        ciphertext: Option<Vec<u8>>,
        content: Vec<u8>,
        outer_content_length: Option<u16>,
        inner_plain_text: Option<Vec<u8>>,
        record_type: RecordType,
        padding_len: usize,
        outer_type: RecordType,
        outer_version: u16,
        ciphertext_len: Option<usize>,
    }

    impl Default for EncryptedInnerArgs {
        fn default() -> Self {
            Self {
                outer_trailer: vec![0u8; 0],
                ciphertext: None,
                content: vec![1, 2, 3, 4, 5],
                outer_content_length: None,
                inner_plain_text: None,
                record_type: RecordType::ApplicationData,
                padding_len: 0,
                outer_type: RecordType::ApplicationData,
                outer_version: VERSION_TLS_1_2,
                ciphertext_len: None,
            }
        }
    }

    impl Encryptor {
        fn make_encrypted_inner_plain_text(&mut self, args: &EncryptedInnerArgs) -> Vec<u8> {
            let mut inner_plaintext = Vec::new();
            if let Some(inner_plain_text) = &args.inner_plain_text {
                inner_plaintext.extend_from_slice(inner_plain_text);
            } else {
                inner_plaintext.extend_from_slice(&args.content);
                inner_plaintext.extend_from_slice(&(args.record_type as u8).to_be_bytes());
                let padding = vec![0u8; args.padding_len];
                inner_plaintext.extend_from_slice(&padding);
            }
            let ciphertext_len = inner_plaintext.len() + 16;
            let mut additional_data = Vec::new();
            additional_data.extend_from_slice(&(args.outer_type as u8).to_be_bytes());
            additional_data.extend_from_slice(&args.outer_version.to_be_bytes());
            let ciphertext_len = if let Some(len) = &args.ciphertext_len {
                *len
            } else {
                ciphertext_len
            } as u16;
            additional_data.extend_from_slice(&ciphertext_len.to_be_bytes());
            self.encrypt(&inner_plaintext, &additional_data).unwrap()
        }

        fn make_encrypted_record(&mut self, args: EncryptedInnerArgs) -> Vec<u8> {
            let ciphertext = if let Some(cipher_text) = args.ciphertext {
                cipher_text.to_vec()
            } else {
                self.make_encrypted_inner_plain_text(&args)
            };
            let length = if let Some(len) = args.outer_content_length {
                len
            } else {
                ciphertext.len() as u16
            };
            make_plain_text_record(
                &ciphertext,
                args.outer_type,
                args.outer_version,
                length,
                &args.outer_trailer,
            )
        }
    }

    fn make_plain_text_record(
        content: &[u8],
        record_type: RecordType,
        version: u16,
        content_length: u16,
        trailer: &[u8],
    ) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(&(record_type as u8).to_be_bytes());
        buf.extend_from_slice(&version.to_be_bytes());
        buf.extend_from_slice(&content_length.to_be_bytes());
        buf.extend_from_slice(content);
        buf.extend_from_slice(trailer);
        buf
    }

    fn setup_encrypted_recv_record_layer<T>(callback: T) -> (Encryptor, RecordLayer<T>)
    where
        T: FnMut(&[u8]) -> Result<()>,
    {
        let mut key = vec![0u8; 32];
        rand::fill(&mut key).unwrap();
        let encryptor = Encryptor::new(&key).unwrap();
        let mut record_layer = RecordLayer::new(callback);
        record_layer.set_recv_key(&key).unwrap();
        (encryptor, record_layer)
    }

    fn setup_decrypt_send_record_layer<T>(callback: T) -> (Decryptor, RecordLayer<T>)
    where
        T: FnMut(&[u8]) -> Result<()>,
    {
        let mut key = vec![0u8; 32];
        rand::fill(&mut key).unwrap();
        let decryptor = Decryptor::new(&key).unwrap();
        let mut record_layer = RecordLayer::new(callback);
        record_layer.set_send_key(&key).unwrap();
        (decryptor, record_layer)
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = vec![0u8; 32];
        let mut es = Encryptor::new(&key).unwrap();
        let additional_data = vec![0u8; 12];
        let encrypted = es.encrypt(SERVER_RAW_APP_DATA, &additional_data).unwrap();
        let mut ds = Decryptor::new(&key).unwrap();
        let decrypted = ds.decrypt(&encrypted, &additional_data).unwrap();
        assert_eq!(hex::encode(SERVER_RAW_APP_DATA), hex::encode(&decrypted));
    }

    #[test]
    fn test_sequence_number_wrapping() {
        let key = vec![0u8; 32];
        let additional_data = vec![0u8; 12];
        let mut es = Encryptor::new(&key).unwrap();
        let mut ds = Decryptor::new(&key).unwrap();
        es.seq_num = MAX_SEQUENCE_NUMBER - 1;
        let encrypted = es.encrypt(SERVER_RAW_APP_DATA, &additional_data).unwrap();
        assert_eq!(es.seq_num, MAX_SEQUENCE_NUMBER);
        assert!(es.encrypt(SERVER_RAW_APP_DATA, &additional_data).is_err());

        ds.seq_num = MAX_SEQUENCE_NUMBER;
        assert!(ds.decrypt(&encrypted, &additional_data).is_err());
    }

    #[test]
    fn test_record_layer_send() {
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::Handshake, SERVER_RAW_APP_DATA)
            .unwrap();
        assert_eq!(sent_data.borrow().len(), 0);
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        let data = sent_data.borrow()[0].clone();
        assert_eq!(data[0], RecordType::Handshake as u8);
        assert_eq!(data[1], 0x03);
        assert_eq!(data[2], 0x03);
        assert_eq!(data[3], 0);
        assert_eq!(data[4], 11);
        assert_eq!(hex::encode(&data[5..]), hex::encode(SERVER_RAW_APP_DATA));
    }

    #[test]
    fn does_not_send_if_no_data() {
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 0);
    }

    #[test]
    fn test_combines_multiple_sends_same_type() {
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::Handshake, b"hello world")
            .unwrap();
        record_layer
            .send(RecordType::Handshake, b"hello again")
            .unwrap();
        assert_eq!(sent_data.borrow().len(), 0);
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        let data = sent_data.borrow()[0].clone();
        assert_eq!(data[0], 22);
        assert_eq!(data[1], 0x03);
        assert_eq!(data[2], 0x03);
        assert_eq!(data[3], 0);
        assert_eq!(data[4], 22);
        assert_eq!(
            hex::encode(&data[5..]),
            hex::encode(b"hello worldhello again")
        );
    }

    #[test]
    fn test_does_not_send_data_that_exceeds_limit() {
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        let too_big = vec![0u8; (MAX_RECORD_SIZE + 1) as usize];
        assert!(record_layer.send(RecordType::Handshake, &too_big).is_err());
    }

    #[test]
    fn test_flush_multiple_when_combined_is_too_big() {
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::Handshake, SERVER_RAW_APP_DATA)
            .unwrap();
        assert_eq!(sent_data.borrow().len(), 0);
        let zeros = vec![0u8; (MAX_RECORD_SIZE - 1) as usize];
        record_layer.send(RecordType::Handshake, &zeros).unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 2);
        let first = sent_data.borrow()[0].clone();
        let second = sent_data.borrow()[1].clone();
        assert_eq!(hex::encode(&first[5..]), hex::encode(SERVER_RAW_APP_DATA));
        assert_eq!(hex::encode(&second[5..10]), "0000000000");
    }

    #[test]
    fn test_send_encrypted_handshake() {
        let sent_data = RefCell::new(Vec::new());
        let (mut decryptor, mut record_layer) = setup_decrypt_send_record_layer(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::Handshake, b"hello world")
            .unwrap();
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        let data = sent_data.borrow()[0].clone();
        assert_eq!(data[0], 23);
        assert_eq!(data[1], 0x03);
        assert_eq!(data[2], 0x03);
        assert_eq!(data[3], 0);
        assert_eq!(data[4], 11 + 1 + 16);
        let ciphertext = &data[5..];
        assert_eq!(ciphertext.len(), 11 + 1 + 16);
        let (content, record_type) = decryptor.decrypt_inner_plain_text(&data);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "hello world");
        assert_eq!(record_type, RecordType::Handshake);
    }

    #[test]
    fn test_send_encrypted_app_data() {
        let sent_data = RefCell::new(Vec::new());
        let (mut decryptor, mut record_layer) = setup_decrypt_send_record_layer(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::ApplicationData, b"hello world")
            .unwrap();
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        let data = sent_data.borrow()[0].clone();
        assert_eq!(data[0], 23);
        assert_eq!(data[1], 0x03);
        assert_eq!(data[2], 0x03);
        assert_eq!(data[3], 0);
        assert_eq!(data[4], 11 + 1 + 16);
        let ciphertext = &data[5..];
        assert_eq!(ciphertext.len(), 11 + 1 + 16);
        let (content, record_type) = decryptor.decrypt_inner_plain_text(&data);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "hello world");
        assert_eq!(record_type, RecordType::ApplicationData);
    }

    #[test]
    fn test_flushes_multiple_with_diff_types() {
        let sent_data = RefCell::new(Vec::new());
        let (mut decryptor, mut record_layer) = setup_decrypt_send_record_layer(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        record_layer
            .send(RecordType::Handshake, b"handshake")
            .unwrap();
        record_layer
            .send(RecordType::Handshake, b"handshake")
            .unwrap();
        record_layer
            .send(RecordType::ApplicationData, b"app-data")
            .unwrap();
        assert_eq!(sent_data.borrow().len(), 1);
        record_layer.flush().unwrap();
        assert_eq!(sent_data.borrow().len(), 2);
        let handshakes = sent_data.borrow()[0].clone();
        let app_data = sent_data.borrow()[1].clone();
        assert_eq!(handshakes[0], 23);
        assert_eq!(handshakes[1], 0x03);
        assert_eq!(handshakes[2], 0x03);
        assert_eq!(handshakes[3], 0);
        assert_eq!(handshakes[4], 18 + 1 + 16);
        let (content, record_type) = decryptor.decrypt_inner_plain_text(&handshakes);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "handshakehandshake");
        assert_eq!(record_type, RecordType::Handshake);

        assert_eq!(app_data[0], 23);
        assert_eq!(app_data[1], 0x03);
        assert_eq!(app_data[2], 0x03);
        assert_eq!(app_data[3], 0);
        assert_eq!(app_data[4], 8 + 1 + 16);
        let (content, record_type) = decryptor.decrypt_inner_plain_text(&app_data);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "app-data");
        assert_eq!(record_type, RecordType::ApplicationData);
    }

    #[test]
    fn test_no_initial_decrypt_state() {
        let sent_data = RefCell::new(Vec::new());
        let record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        assert!(record_layer.recv_decryption_state.is_none())
    }

    #[test]
    fn test_accepts_plaintext_handshake() {
        let record = make_plain_text_record(
            &vec![1, 2, 3, 4, 5],
            RecordType::Handshake,
            VERSION_TLS_1_2,
            5,
            &vec![0u8; 0],
        );
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        let (record_type, content) = record_layer.recv(&record).unwrap();
        assert_eq!(record_type, RecordType::Handshake);
        assert_eq!(hex::encode(&content), hex::encode(&vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_accepts_legacy_version_number() {
        let expected_content = vec![1, 2, 3, 4, 5];
        let record = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            0x0301,
            5,
            &vec![0u8; 0],
        );
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        let (record_type, content) = record_layer.recv(&record).unwrap();
        assert_eq!(record_type, RecordType::Handshake);
        assert_eq!(hex::encode(&content), hex::encode(&expected_content));
    }

    #[test]
    fn test_rejects_unknown_version() {
        let expected_content = vec![1, 2, 3, 4, 5];
        let record_1 = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            0x0000,
            5,
            &vec![0u8; 0],
        );
        let record_2 = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            0x1234,
            5,
            &vec![0u8; 0],
        );

        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        assert!(record_layer.recv(&record_1).is_err());
        assert!(record_layer.recv(&record_2).is_err());
    }

    #[test]
    fn test_reject_too_large() {
        let expected_content = vec![1, 2, 3, 4, 5];
        let expected_content_2 = vec![0u8; MAX_RECORD_SIZE as usize + 1];
        let record_1 = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            VERSION_TLS_1_2,
            MAX_RECORD_SIZE,
            &vec![0u8; 0],
        );
        let record_2 = make_plain_text_record(
            &expected_content_2,
            RecordType::Handshake,
            VERSION_TLS_1_2,
            MAX_RECORD_SIZE + 1,
            &vec![0u8; 0],
        );
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        assert!(record_layer.recv(&record_1).is_err());
        assert!(record_layer.recv(&record_2).is_err());
    }

    #[test]
    fn test_reject_trialing_data() {
        let expected_content = vec![1, 2, 3, 4, 5];
        let record = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            VERSION_TLS_1_2,
            5,
            &vec![0u8; 12],
        );
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        assert!(record_layer.recv(&record).is_err());
    }

    #[test]
    fn test_reject_incomplete_record() {
        let expected_content = vec![1, 2, 3, 4, 5];
        let record = make_plain_text_record(
            &expected_content,
            RecordType::Handshake,
            VERSION_TLS_1_2,
            5,
            &vec![0u8; 0],
        );
        let sent_data = RefCell::new(Vec::new());
        let mut record_layer = RecordLayer::new(|data: &[u8]| {
            sent_data.borrow_mut().push(data.to_vec());
            Ok(())
        });
        assert!(record_layer.recv(&record[..record.len() - 1]).is_err());
    }

    #[test]
    fn test_accept_encrypted_records() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let args = EncryptedInnerArgs::default();
        let encrypted_record = encryptor.make_encrypted_record(args);
        let (record_type, content) = record_layer.recv(&encrypted_record).unwrap();
        assert_eq!(record_type, RecordType::ApplicationData);
        assert_eq!(hex::encode(content), hex::encode(vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_accept_encrypted_handshake() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.record_type = RecordType::Handshake;
        let encrypted_record = encryptor.make_encrypted_record(args);
        let (record_type, content) = record_layer.recv(&encrypted_record).unwrap();
        assert_eq!(record_type, RecordType::Handshake);
        assert_eq!(hex::encode(content), hex::encode(vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_accept_empty_app_data() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.content = vec![0u8; 0];
        let encrypted_record = encryptor.make_encrypted_record(args);
        let (record_type, content) = record_layer.recv(&encrypted_record).unwrap();
        assert_eq!(record_type, RecordType::ApplicationData);
        assert_eq!(hex::encode(content), hex::encode(vec![0u8; 0]));
    }

    #[test]
    fn test_correctly_strips_padding() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let pad_length = 12;
        let mut args = EncryptedInnerArgs::default();
        args.content = b"hello world".to_vec();
        args.padding_len = pad_length;
        let padded_cipher_text = encryptor.make_encrypted_inner_plain_text(&args);
        let mut args2 = EncryptedInnerArgs::default();
        args2.content = b"hello world".to_vec();
        let unpadded_cipher_text = encryptor.make_encrypted_inner_plain_text(&args2);
        assert_eq!(
            padded_cipher_text.len() - unpadded_cipher_text.len(),
            pad_length
        );
        let mut arg3 = EncryptedInnerArgs::default();
        arg3.ciphertext = Some(padded_cipher_text);
        let encrypted_record = encryptor.make_encrypted_record(arg3);
        let (record_type, content) = record_layer.recv(&encrypted_record).unwrap();
        assert_eq!(record_type, RecordType::ApplicationData);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "hello world");
    }

    #[test]
    fn test_strips_padding_empty() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let pad_length = 12;
        let mut args = EncryptedInnerArgs::default();
        args.content = b"".to_vec();
        args.padding_len = pad_length;
        let padded_cipher_text = encryptor.make_encrypted_inner_plain_text(&args);
        let mut args2 = EncryptedInnerArgs::default();
        args2.content = b"".to_vec();
        let unpadded_cipher_text = encryptor.make_encrypted_inner_plain_text(&args2);
        assert_eq!(
            padded_cipher_text.len() - unpadded_cipher_text.len(),
            pad_length
        );
        let mut arg3 = EncryptedInnerArgs::default();
        arg3.ciphertext = Some(padded_cipher_text);
        let encrypted_record = encryptor.make_encrypted_record(arg3);
        let (record_type, content) = record_layer.recv(&encrypted_record).unwrap();
        assert_eq!(record_type, RecordType::ApplicationData);
        assert_eq!(std::str::from_utf8(&content).unwrap(), "");
    }

    #[test]
    fn test_refuses_data_after_record() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.outer_trailer = vec![0u8; 12];
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_refuses_partial_record() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let args = EncryptedInnerArgs::default();
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer
            .recv(&encrypted_record[..encrypted_record.len() - 1])
            .is_err());
    }

    #[test]
    fn test_refuses_encrypted_change_cipher_spec() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.record_type = RecordType::ChangeCipherSpec;
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_rejects_unknown_version_recv() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.outer_version = 0x0000;
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
        let mut args = EncryptedInnerArgs::default();
        args.outer_version = 0x1234;
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_rejects_legacy_version_number() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.outer_version = 0x0301;
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_rejects_outer_not_application() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.outer_type = RecordType::Handshake;
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_rejects_too_large_recv() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.outer_content_length = Some(MAX_ENCRYPTED_RECORD_SIZE);
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
        let mut args = EncryptedInnerArgs::default();
        args.outer_content_length = Some(MAX_ENCRYPTED_RECORD_SIZE + 1);
        args.ciphertext = Some(vec![0u8; (MAX_ENCRYPTED_RECORD_SIZE + 1) as usize]);
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_reject_all_padding() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.inner_plain_text = Some(vec![0u8; 7]);
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_reject_tampered() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.content = b"hello world".to_vec();
        let mut ciphertext = encryptor.make_encrypted_inner_plain_text(&args);
        ciphertext[0] += 1;
        args.ciphertext = Some(ciphertext);
        let encrypted_record = encryptor.make_encrypted_record(args);
        assert!(record_layer.recv(&encrypted_record).is_err());
    }

    #[test]
    fn test_additonal_tampered() {
        let (mut encryptor, mut record_layer) =
            setup_encrypted_recv_record_layer(|_: &[u8]| Ok(()));
        let mut args = EncryptedInnerArgs::default();
        args.content = b"hello world".to_vec();
        args.outer_version = 0x0301;
        let mut encrypted_record = encryptor.make_encrypted_record(args);
        encrypted_record[1] = 0x03;
        encrypted_record[2] = 0x03;
        assert!(record_layer.recv(&encrypted_record).is_err());
    }
}
