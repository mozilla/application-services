uniffi::setup_scaffolding!();

/// Initialization of the megazord crate. Must be called before any other calls to rust components.
#[uniffi::export]
pub fn initialize() {
    // this is currently empty, we will add nss initialization code here in a next step
}
