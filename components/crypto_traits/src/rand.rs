//! Cryptographic random number generator
//!
pub trait Rand {
    type Error: std::error::Error;

    /// Will generate random bytes equal to the size of the `res` slice
    fn rand(&self, res: &mut [u8]) -> std::result::Result<(), Self::Error>;
}
