//! # Crypto Traits
//! A collection of cryptography traits used by Application Services components.
//!
//! ## Goal
//! The goal of this crate is to provide backend-agnostic traits, that can be implemented by multiple backends.
//!
//! The need for this crate arises because most crates that need cryptography is Application Services utilize rc_crypto,
//! which provides NSS-backed cryptography. However, some of those crates might be used in non-Firefox environments, for example,
//! the FxA client could be used on the web as a wasm module. Thus, a critical goal of this crate is to enable a Web API backend for cryptography.
//!
//! Each trait is split into its own module, for modularity but also so consumer can choose which cryptography functionality they would like
//!

use aead::Aead;
use digest::Digest;
use hawk::Hawk;
use hkdf::Hkdf;
use hmac::Hmac;
pub mod aead;
pub mod digest;
pub mod error;
pub mod hawk;
pub mod hkdf;
pub mod hmac;
pub mod rand;
pub use error::Error;
use rand::Rand;

pub trait Cryptographer: Aead + Hkdf + Hmac + Digest + Hawk + Rand + Sync + Send {}
static CRYPTOGRAPHER: std::sync::OnceLock<&'static dyn Cryptographer> = std::sync::OnceLock::new();
pub fn set_cryptographer(crypto: &'static dyn Cryptographer) -> error::Result<()> {
    CRYPTOGRAPHER
        .set(crypto)
        .map_err(|_| Error::UnableToSetCryptographer)
}

pub fn get_cryptographer() -> error::Result<&'static dyn Cryptographer> {
    CRYPTOGRAPHER
        .get()
        .copied()
        .ok_or_else(|| Error::CryptographerNotSet)
}
