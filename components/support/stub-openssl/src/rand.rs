use crate::error::{fatal, ErrorStack};

pub fn rand_bytes(_: &mut [u8]) -> Result<(), ErrorStack> {
    fatal()
}
