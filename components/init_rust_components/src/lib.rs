use nss::ensure_initialized as ensure_nss_initialized;

uniffi::setup_scaffolding!();

/// Initialization of the megazord crate. Must be called before any other calls to rust components.
#[uniffi::export]
pub fn initialize() {
    ensure_nss_initialized();
}
