//! HAWK
//!
pub trait Hawk: hawk::crypto::Cryptographer + Sized {
    /// **Important**: Due to how the Hawk crate's API works, consumers **must**
    /// call this function before using any hawk API's. Luckily, hawk will panic if used
    /// before this is called, making it difficult to forget calling this function in development/testing.
    ///
    /// Using this API can be tricky since it requires a static lifetime on `self`. If your cryptographer is not leaked
    /// (i.e, it is not already static), you might want to intentionally leak it using [`Box::leak`]
    /// ```rs
    /// use crypto_traits::hawk::Hawk;
    /// pub struct NSSCryptographer;
    /// // Implement the `hawk::Cryptographer` trait here
    ///
    /// fn main() {
    ///   let crypto = Box::new(NSSCryptographer);
    ///   Box::leak(crypto).set_cryptographer();
    /// }
    ///
    /// ```
    fn set_cryptographer(&'static self) {
        let _ = hawk::crypto::set_cryptographer(self).ok(); // errors if the cryptographer is already set
    }
}
pub use hawk::*;
