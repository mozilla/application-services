use crate::error::{fatal, ErrorStack};

#[derive(Clone, Copy)]
pub struct Cipher {}

pub struct Crypter {}

pub enum Mode {
    Encrypt,
    Decrypt,
}

impl Cipher {
    pub fn aes_128_gcm() -> Self {
        fatal()
    }

    pub fn block_size(&self) -> usize {
        fatal()
    }
}

impl Crypter {
    pub fn new(_: Cipher, _: Mode, _: &[u8], _: Option<&[u8]>) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn update(&self, _: &[u8], _: &mut [u8]) -> Result<usize, ErrorStack> {
        fatal()
    }

    pub fn finalize(&self, _: &mut [u8]) -> Result<usize, ErrorStack> {
        fatal()
    }

    pub fn get_tag(&self, _: &mut [u8]) -> Result<usize, ErrorStack> {
        fatal()
    }

    pub fn set_tag(&self, _: &[u8]) -> Result<(), ErrorStack> {
        fatal()
    }
}
