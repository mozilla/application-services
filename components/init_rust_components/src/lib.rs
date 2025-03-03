#[cfg(not(feature = "keydb"))]
use nss::ensure_initialized as ensure_nss_initialized;
#[cfg(feature = "keydb")]
use nss::ensure_initialized_with_profile_dir as ensure_nss_initialized_with_profile_dir;

uniffi::setup_scaffolding!();

/// Global initialization routines for Rust components. Must be called before any other calls to
/// Rust components.
///
/// For adding additional initialization code: Note that this function is called very early in the
/// app lifetime and therefore affects the startup time. Only the most necessary things should be
/// done here.
#[cfg(not(feature = "keydb"))]
#[uniffi::export]
pub fn initialize() {
    ensure_nss_initialized();
}

/// Global initialization routines for Rust components, when `keydb` feature is activated. Must be
/// called before any other calls to Rust components.
///
/// Receives the path to the profile directory.
///
/// For adding additional initialization code: Note that this function is called very early in the
/// app lifetime and therefore affects the startup time. Only the most necessary things should be
/// done here.
#[cfg(feature = "keydb")]
#[uniffi::export]
pub fn initialize(profile_path: String) {
    ensure_nss_initialized_with_profile_dir(profile_path).expect("could not initialize NSS")
}
