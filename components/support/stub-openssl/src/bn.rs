use crate::error::{fatal, ErrorStack};

pub struct BigNum {}
pub struct BigNumContext {}

impl BigNum {
    pub fn new() -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        fatal()
    }

    pub fn from_slice(_: &[u8]) -> Result<Self, ErrorStack> {
        fatal()
    }
}

impl BigNumContext {
    pub fn new() -> Result<Self, ErrorStack> {
        fatal()
    }
}
