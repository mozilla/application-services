use crate::error::{fatal, ErrorStack};
use crate::pkey::{PKey, Private, Public};

pub struct Deriver {}

impl Deriver {
    pub fn new(_: &PKey<Private>) -> Result<Self, ErrorStack> {
        fatal()
    }

    pub fn set_peer(&self, _: &PKey<Public>) -> Result<(), ErrorStack> {
        fatal()
    }

    pub fn derive_to_vec(&self) -> Result<Vec<u8>, ErrorStack> {
        fatal()
    }
}
