use std::collections::HashMap;

use crate::{mars::MARSClient, MozAdsPlacementConfig};

#[derive(uniffi::Object)]
pub struct MozAdsClient {
    mars_client: MARSClient,
    registered_placements: HashMap<String, MozAdsPlacementConfig>,
}

#[uniffi::export]
impl MozAdsClient {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            mars_client: MARSClient::new(),
            registered_placements: HashMap::new(),
        }
    }
}
