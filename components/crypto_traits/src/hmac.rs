//! HMAC Keyed-Hashing for Message Authentication
//!
//! Trait for HMAC, see <https://datatracker.ietf.org/doc/html/rfc2104> for details
//!

use crate::{
    digest::{Digest, HashAlgorithm},
    error,
};
pub trait Hmac: Digest {
    fn sign(&self, algorithm: HashAlgorithm, key: &[u8], data: &[u8]) -> error::Result<Vec<u8>>;
    fn verify(
        &self,
        algorithm: HashAlgorithm,
        key: &[u8],
        data: &[u8],
        signature: &[u8],
    ) -> error::Result<()>;
}
