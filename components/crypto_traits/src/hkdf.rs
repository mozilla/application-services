//! HKDF
//!

use crate::{digest::HashAlgorithm, error, hmac::Hmac};
pub trait Hkdf: Hmac {
    fn extract(
        &self,
        digest_algorithm: HashAlgorithm,
        salt: &[u8],
        secret: &[u8],
    ) -> error::Result<Vec<u8>>;

    fn expand(
        &self,
        digest_algorithm: HashAlgorithm,
        prk: &[u8],
        info: &[u8],
        out: &mut [u8],
    ) -> error::Result<()>;

    fn extract_and_expand(
        &self,
        digest_algorithm: HashAlgorithm,
        salt: &[u8],
        secret: &[u8],
        info: &[u8],
        out: &mut [u8],
    ) -> error::Result<()> {
        let prk = self.extract(digest_algorithm, salt, secret)?;
        self.expand(digest_algorithm, &prk, info, out)
    }
}
