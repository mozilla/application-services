/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::utils;
use super::utils::HASH_LENGTH;
use crate::pairing_channel::messages::HandshakeType;
use anyhow::Result;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::convert::TryInto;

pub enum ExtensionType {
    PreSharedKey,
    SupportedVersions,
    PskKeyExchangeModes,
    UnspecifiedExtension,
}

impl Into<u16> for ExtensionType {
    fn into(self) -> u16 {
        match self {
            ExtensionType::PreSharedKey => 41,
            ExtensionType::SupportedVersions => 43,
            ExtensionType::PskKeyExchangeModes => 45,
            ExtensionType::UnspecifiedExtension => 0,
        }
    }
}

impl From<u16> for ExtensionType {
    fn from(num: u16) -> Self {
        match num {
            41 => ExtensionType::PreSharedKey,
            43 => ExtensionType::SupportedVersions,
            45 => ExtensionType::PskKeyExchangeModes,
            _ => ExtensionType::UnspecifiedExtension,
        }
    }
}

trait ExtensionValue: std::fmt::Debug {
    fn get_type_tag(&self) -> u16;
    fn write(&self, message_type: HandshakeType, buf: &mut BytesMut) -> Result<()>;
    fn contains_version(&self, version: u16) -> bool;
    fn is_selected_version(&self, version: u16) -> bool;
    fn get_selected_identity(&self) -> Result<u16>;
    fn get_modes(&self) -> Result<Vec<u8>>;
    fn get_identities(&self) -> Result<Vec<Vec<u8>>>;
    fn get_binders(&self) -> Result<Vec<Vec<u8>>>;
}

#[derive(Debug)]
enum PreSharedKeyState {
    ClientHello {
        identities: Vec<Vec<u8>>,
        binders: Vec<Vec<u8>>,
    },
    ServerHello {
        selected_identity: u16,
    },
}
// The PreSharedKey extension:
//
//  struct {
//    opaque identity<1..2^16-1>;
//    uint32 obfuscated_ticket_age;
//  } PskIdentity;
//  opaque PskBinderEntry<32..255>;
//  struct {
//    PskIdentity identities<7..2^16-1>;
//    PskBinderEntry binders<33..2^16-1>;
//  } OfferedPsks;
//  struct {
//    select(Handshake.msg_type) {
//      case client_hello: OfferedPsks;
//      case server_hello: uint16 selected_identity;
//    };
//  } PreSharedKeyExtension;
#[derive(Debug)]
pub struct PreSharedKeyExtension {
    state: PreSharedKeyState,
}

impl ExtensionValue for PreSharedKeyExtension {
    fn get_binders(&self) -> Result<Vec<Vec<u8>>> {
        if let PreSharedKeyState::ClientHello {
            identities: _,
            binders,
        } = &self.state
        {
            Ok(binders.clone())
        } else {
            anyhow::bail!("Invalid state")
        }
    }
    fn get_identities(&self) -> Result<Vec<Vec<u8>>> {
        if let PreSharedKeyState::ClientHello {
            identities,
            binders: _,
        } = &self.state
        {
            Ok(identities.clone())
        } else {
            anyhow::bail!("Invalid state")
        }
    }
    fn get_modes(&self) -> Result<Vec<u8>> {
        anyhow::bail!("Invalid ext");
    }
    fn get_selected_identity(&self) -> Result<u16> {
        if let PreSharedKeyState::ServerHello { selected_identity } = &self.state {
            Ok(*selected_identity)
        } else {
            anyhow::bail!("Invalid state");
        }
    }
    fn get_type_tag(&self) -> u16 {
        ExtensionType::PreSharedKey.into()
    }

    fn write(&self, message_type: HandshakeType, data_buf: &mut BytesMut) -> Result<()> {
        match message_type {
            HandshakeType::ClientHello => {
                if let PreSharedKeyState::ClientHello {
                    identities,
                    binders,
                } = &self.state
                {
                    utils::write_with_callback_u16_len(data_buf, &mut |identities_buf| {
                        identities.iter().try_for_each(|identity| -> Result<()> {
                            identities_buf.put_u16(identity.len().try_into()?);
                            identities_buf.put(identity.as_slice());
                            identities_buf.put_u32(0); // tag age
                            Ok(())
                        })?;
                        Ok(())
                    })?;
                    utils::write_with_callback_u16_len(data_buf, &mut |binders_buf| {
                        binders.iter().try_for_each(|binder| -> Result<()> {
                            binders_buf.put_u8(binder.len().try_into()?);
                            binders_buf.put(binder.as_slice());
                            Ok(())
                        })?;
                        Ok(())
                    })?;
                } else {
                    anyhow::bail!("Not a preshared key extenstion");
                }
            }
            HandshakeType::ServerHello => {
                if let PreSharedKeyState::ServerHello { selected_identity } = &self.state {
                    data_buf.put_u16(*selected_identity);
                } else {
                    anyhow::bail!("Invalid preshared key state")
                }
            }
            _ => anyhow::bail!("Invalid handshake type"),
        };
        Ok(())
    }

    fn contains_version(&self, _: u16) -> bool {
        false
    }

    fn is_selected_version(&self, _: u16) -> bool {
        false
    }
}

impl PreSharedKeyExtension {
    #[allow(dead_code)]
    pub fn from_identities_and_binders(identities: Vec<Vec<u8>>, binders: Vec<Vec<u8>>) -> Self {
        Self {
            state: PreSharedKeyState::ClientHello {
                identities,
                binders,
            },
        }
    }

    pub fn from_selected_identity(selected_identity: u16) -> Self {
        Self {
            state: PreSharedKeyState::ServerHello { selected_identity },
        }
    }
    fn read(message_type: HandshakeType, buff_data: &mut Bytes) -> Result<Self> {
        match message_type {
            HandshakeType::ClientHello => {
                let mut identities = Vec::new();
                let mut binders = Vec::new();
                let identities_buff = utils::read_bytes_with_u16_len(buff_data)?;
                let mut identities_buff = identities_buff.as_slice().to_bytes();
                utils::read_callback_till_done(&mut identities_buff, &mut |buf| {
                    let identity = utils::read_bytes_with_u16_len(buf)?;
                    utils::read_u32(buf)?;
                    identities.push(identity);
                    Ok(())
                })?;
                let binders_buff = utils::read_bytes_with_u16_len(buff_data)?;
                let mut binders_buff = binders_buff.as_slice().to_bytes();
                utils::read_callback_till_done(&mut binders_buff, &mut |data_buf| {
                    let binder = utils::read_bytes_with_u8_len(data_buf)?;
                    if binder.len() < HASH_LENGTH {
                        anyhow::bail!("Binder too small!");
                    }
                    binders.push(binder);
                    Ok(())
                })?;
                Ok(Self {
                    state: PreSharedKeyState::ClientHello {
                        identities,
                        binders,
                    },
                })
            }
            HandshakeType::ServerHello => {
                let selected_identity = utils::read_u16(buff_data)?;
                Ok(Self {
                    state: PreSharedKeyState::ServerHello { selected_identity },
                })
            }
            _ => anyhow::bail!("Invalid Handshake Type!"),
        }
    }
}

#[derive(Debug)]
enum SupportedVersionState {
    ClientHello { versions: Vec<u16> },
    ServerHello { selected_version: u16 },
}
// The SupportedVersions extension:
//
//  struct {
//    select(Handshake.msg_type) {
//      case client_hello:
//        ProtocolVersion versions < 2..254 >;
//      case server_hello:
//        ProtocolVersion selected_version;
//    };
//  } SupportedVersions;
#[derive(Debug)]
pub struct SupportedVersionsExtension {
    state: SupportedVersionState,
}

impl ExtensionValue for SupportedVersionsExtension {
    fn get_identities(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }
    fn get_binders(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }
    fn get_modes(&self) -> Result<Vec<u8>> {
        anyhow::bail!("Invalid ext");
    }

    fn get_selected_identity(&self) -> Result<u16> {
        anyhow::bail!("Invalid extension");
    }
    fn get_type_tag(&self) -> u16 {
        ExtensionType::SupportedVersions.into()
    }

    fn write(&self, message_type: HandshakeType, data_buf: &mut BytesMut) -> Result<()> {
        match message_type {
            HandshakeType::ClientHello => {
                utils::write_with_callback_u8_len(data_buf, &mut |versions_buf| {
                    if let SupportedVersionState::ClientHello { versions } = &self.state {
                        versions.iter().for_each(|identity| {
                            versions_buf.put_u16(*identity);
                        });
                    } else {
                        anyhow::bail!("Not a supported versions extenstion");
                    }
                    Ok(())
                })?;
            }
            HandshakeType::ServerHello => {
                if let SupportedVersionState::ServerHello { selected_version } = &self.state {
                    data_buf.put_u16(*selected_version);
                } else {
                    anyhow::bail!("State should be server hello")
                }
            }
            _ => anyhow::bail!("Invalid handshake type"),
        };
        Ok(())
    }

    fn contains_version(&self, version: u16) -> bool {
        if let SupportedVersionState::ClientHello { versions } = &self.state {
            versions.iter().any(|v| *v == version)
        } else {
            false
        }
    }

    fn is_selected_version(&self, version: u16) -> bool {
        if let SupportedVersionState::ServerHello { selected_version } = &self.state {
            *selected_version == version
        } else {
            false
        }
    }
}

impl SupportedVersionsExtension {
    #[allow(dead_code)]
    pub fn from_versions(versions: Vec<u16>) -> Self {
        Self {
            state: SupportedVersionState::ClientHello { versions },
        }
    }

    pub fn from_selected_version(selected_version: u16) -> Self {
        Self {
            state: SupportedVersionState::ServerHello { selected_version },
        }
    }

    fn read(message_type: HandshakeType, buf: &mut Bytes) -> Result<Self> {
        match message_type {
            HandshakeType::ClientHello => {
                let mut versions = Vec::new();
                let versions_buf = utils::read_bytes_with_u8_len(buf)?;
                let mut versions_buf = versions_buf.as_slice().to_bytes();
                utils::read_callback_till_done(&mut versions_buf, &mut |buf| {
                    versions.push(utils::read_u16(buf)?);
                    Ok(())
                })?;
                Ok(Self {
                    state: SupportedVersionState::ClientHello { versions },
                })
            }
            HandshakeType::ServerHello => {
                let selected_version = utils::read_u16(buf)?;
                Ok(Self {
                    state: SupportedVersionState::ServerHello { selected_version },
                })
            }
            _ => anyhow::bail!("Invalid handshake message"),
        }
    }
}

impl From<SupportedVersionsExtension> for Extension {
    fn from(supported_version_ext: SupportedVersionsExtension) -> Self {
        Self {
            extension: Box::new(supported_version_ext),
        }
    }
}

impl From<PreSharedKeyExtension> for Extension {
    fn from(pre_shared_ext: PreSharedKeyExtension) -> Self {
        Self {
            extension: Box::new(pre_shared_ext),
        }
    }
}

impl From<PskKeyExchangeExtension> for Extension {
    fn from(psk_key_exchange_ext: PskKeyExchangeExtension) -> Self {
        Self {
            extension: Box::new(psk_key_exchange_ext),
        }
    }
}

#[derive(Debug)]
pub struct PskKeyExchangeExtension {
    modes: Vec<u8>,
}

impl ExtensionValue for PskKeyExchangeExtension {
    fn get_binders(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }

    fn get_identities(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }
    fn get_modes(&self) -> Result<Vec<u8>> {
        Ok(self.modes.clone())
    }
    fn get_selected_identity(&self) -> Result<u16> {
        anyhow::bail!("Invalid extension");
    }
    fn get_type_tag(&self) -> u16 {
        ExtensionType::PskKeyExchangeModes.into()
    }

    fn write(&self, message_type: HandshakeType, buf: &mut BytesMut) -> Result<()> {
        match message_type {
            HandshakeType::ClientHello => {
                utils::write_with_callback_u8_len(buf, &mut |modes_buf| {
                    self.modes.iter().for_each(|mode| {
                        modes_buf.put_u8(*mode);
                    });
                    Ok(())
                })?
            }
            _ => anyhow::bail!("Invalid handshake type"),
        };
        Ok(())
    }

    fn contains_version(&self, _: u16) -> bool {
        false
    }

    fn is_selected_version(&self, _: u16) -> bool {
        false
    }
}

impl PskKeyExchangeExtension {
    #[allow(dead_code)]
    pub fn new(modes: Vec<u8>) -> Self {
        Self { modes }
    }
    fn read(message_type: HandshakeType, buf: &mut Bytes) -> Result<Self> {
        match message_type {
            HandshakeType::ClientHello => {
                let mut modes = Vec::new();
                let modes_buf = utils::read_bytes_with_u8_len(buf)?;
                let mut modes_buf = modes_buf.as_slice().to_bytes();
                utils::read_callback_till_done(&mut modes_buf, &mut |buf| {
                    modes.push(utils::read_u8(buf)?);
                    Ok(())
                })?;
                Ok(Self { modes })
            }
            _ => anyhow::bail!("Invalid Handshake message"),
        }
    }
}

#[derive(Debug)]
struct UnrecognizedExtension {}

impl ExtensionValue for UnrecognizedExtension {
    fn get_binders(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }

    fn get_identities(&self) -> Result<Vec<Vec<u8>>> {
        anyhow::bail!("Invalid Ext")
    }
    fn get_modes(&self) -> Result<Vec<u8>> {
        anyhow::bail!("Invalid ext");
    }
    fn get_selected_identity(&self) -> Result<u16> {
        anyhow::bail!("Invalid extension");
    }
    fn get_type_tag(&self) -> u16 {
        ExtensionType::UnspecifiedExtension.into()
    }

    fn write(&self, _: HandshakeType, _: &mut BytesMut) -> Result<()> {
        Ok(())
    }

    fn contains_version(&self, _: u16) -> bool {
        false
    }

    fn is_selected_version(&self, _: u16) -> bool {
        false
    }
}

impl UnrecognizedExtension {
    fn new() -> Self {
        Self {}
    }
}
// Base struct for generic reading/writing of extensions,
// which are all uniformly formatted as:
//
//   struct {
//     ExtensionType extension_type;
//     opaque extension_data<0..2^16-1>;
//   } Extension;
//
// Extensions always appear inside of a handshake message,
// and their internal structure may differ based on the
// type of that message.
#[derive(Debug)]
pub struct Extension {
    extension: Box<dyn ExtensionValue>,
}

impl Extension {
    pub fn read(message_type: HandshakeType, buf: &mut Bytes) -> Result<Extension> {
        if buf.remaining() < 4 {
            anyhow::bail!("Buffer not large enough!");
        }
        let extenstion_type = utils::read_u16(buf)?;
        let data = utils::read_bytes_with_u16_len(buf)?;
        let mut data = data.as_slice().to_bytes();
        match extenstion_type.into() {
            ExtensionType::PreSharedKey => Ok(Self {
                extension: Box::new(PreSharedKeyExtension::read(message_type, &mut data)?),
            }),
            ExtensionType::SupportedVersions => Ok(Self {
                extension: Box::new(SupportedVersionsExtension::read(message_type, &mut data)?),
            }),
            ExtensionType::PskKeyExchangeModes => Ok(Self {
                extension: Box::new(PskKeyExchangeExtension::read(message_type, &mut data)?),
            }),
            _ => Ok(Self {
                extension: Box::new(UnrecognizedExtension::new()),
            }),
        }
    }

    pub fn get_modes(&self) -> Result<Vec<u8>> {
        self.extension.get_modes()
    }

    pub fn contains_version(&self, version: u16) -> bool {
        self.extension.contains_version(version)
    }

    pub fn is_selected_version(&self, version: u16) -> bool {
        self.extension.is_selected_version(version)
    }

    pub fn get_type_tag(&self) -> u16 {
        self.extension.get_type_tag()
    }

    pub fn get_identities(&self) -> Result<Vec<Vec<u8>>> {
        self.extension.get_identities()
    }

    pub fn write(&self, message_type: HandshakeType, buf: &mut BytesMut) -> Result<()> {
        buf.put_u16(self.get_type_tag());
        utils::write_with_callback_u16_len(buf, &mut |data_buf| {
            self.extension.write(message_type, data_buf)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn get_selected_identity(&self) -> Result<u16> {
        self.extension.get_selected_identity()
    }

    pub fn get_binders(&self) -> Result<Vec<Vec<u8>>> {
        self.extension.get_binders()
    }
}
