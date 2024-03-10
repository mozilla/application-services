//! Cryptographic random number generator
//!
pub trait Rand {
    /// Will generate random bytes equal to the size of the `res` slice
    fn rand(&self, res: &mut [u8]) -> std::result::Result<(), crate::Error>;
}
