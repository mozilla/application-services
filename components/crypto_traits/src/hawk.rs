//! HAWK
//!
pub trait Hawk: hawk::crypto::Cryptographer {}
pub use hawk::*;
