//! Digest, a trait for cryptographers that know how to
//!
//! use various hashing algorithms
//!

use crate::error;
pub trait Digest {
    fn digest(&self, algorithm: HashAlgorithm, data: &[u8]) -> error::Result<Vec<u8>>;
}

#[derive(Debug, Clone, Copy)]
pub enum HashAlgorithm {
    Sha256,
    Sha384,
}
