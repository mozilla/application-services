pub mod builder;
pub mod connection_initializer;
pub mod store;

use crate::{
    ads_store::{builder::AdsStoreBuilder, store::AdsStoreHolder},
    http_cache::ByteSize,
};
use std::path::Path;

/// Identification of placement sent and returned from MARS (eg: `mock_spoc_1`)
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct PlacementId(String);

impl PlacementId {
    pub fn new(s: &str) -> PlacementId {
        PlacementId(s.to_string())
    }
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for PlacementId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub struct AdsStore {
    #[allow(dead_code)]
    max_size: ByteSize,
    holder: AdsStoreHolder,
}

impl AdsStore {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> AdsStoreBuilder {
        AdsStoreBuilder::new(db_path.as_ref())
    }

    pub fn clear(&self) -> Result<(), rusqlite::Error> {
        self.holder.clear_all()?;
        Ok(())
    }

    pub fn shutdown_db(self) -> Result<(), rusqlite::Error> {
        self.holder.close()
    }

    pub fn invalidate_by_id(&self, placement_id: &PlacementId) -> Result<(), rusqlite::Error> {
        self.holder.invalidate_ad_by_id(placement_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::mars::ad_response::{AdCallbacks, AdImage, StorableAd, StorableAdType};
    use url::Url;

    #[test]
    fn test_ads_store_creation() {
        // Test that AdsStore can be created successfully with test config
        let store: Result<AdsStore, _> = AdsStore::builder("test_store.db").build();
        assert!(store.is_ok());
    }

    #[test]
    fn test_clear_store() {
        let store: AdsStore = AdsStore::builder("test_clear.db").build().unwrap();

        // Create a test request and response
        let base_url = mockito::server_url();
        let ad = AdImage {
            url: "https://ads.fakeexample.org/example_ad_1".to_string(),
            image_url: "https://ads.fakeexample.org/example_image_1".to_string(),
            format: "billboard".to_string(),
            block_key: "abc123".into(),
            alt_text: Some("An ad for a puppy".to_string()),
            callbacks: AdCallbacks {
                click: Url::parse(&format!("{}/click/example_ad_1", base_url)).unwrap(),
                impression: Url::parse(&format!("{}/impression/example_ad_1", base_url)).unwrap(),
                report: Some(Url::parse(&format!("{}/report/example_ad_1", base_url)).unwrap()),
            },
        };

        let ad = StorableAd {
            placement_id: PlacementId::new("mock_billboard_1"),
            ad_type: StorableAdType::Image,
            ad_body: serde_json::to_vec(&ad).unwrap(),
        };

        store
            .holder
            .store_with_ttl(ad.clone(), &Duration::from_secs(300))
            .unwrap();

        // Verify it's cached
        let retrieved = store.holder.lookup(&ad.placement_id).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        store.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = store.holder.lookup(&ad.placement_id).unwrap();
        assert!(retrieved_after_clear.is_none());
    }

    #[test]
    fn test_invalidate_by_hash() {
        let store: AdsStore = AdsStore::builder("test_invalidate.db").build().unwrap();

        // Create a test request and response
        let base_url = mockito::server_url();
        let ad = AdImage {
            url: "https://ads.fakeexample.org/example_ad_1".to_string(),
            image_url: "https://ads.fakeexample.org/example_image_1".to_string(),
            format: "billboard".to_string(),
            block_key: "abc123".into(),
            alt_text: Some("An ad for a puppy".to_string()),
            callbacks: AdCallbacks {
                click: Url::parse(&format!("{}/click/example_ad_1", base_url)).unwrap(),
                impression: Url::parse(&format!("{}/impression/example_ad_1", base_url)).unwrap(),
                report: Some(Url::parse(&format!("{}/report/example_ad_1", base_url)).unwrap()),
            },
        };

        let ad_1 = StorableAd {
            placement_id: PlacementId::new("mock_billboard_1"),
            ad_type: StorableAdType::Image,
            ad_body: serde_json::to_vec(&ad).unwrap(),
        };
        let ad_2 = StorableAd {
            placement_id: PlacementId::new("mock_billboard_2"),
            ad_type: StorableAdType::Image,
            ad_body: serde_json::to_vec(&ad).unwrap(),
        };

        store
            .holder
            .store_with_ttl(ad_1.clone(), &Duration::from_secs(300))
            .unwrap();

        store
            .holder
            .store_with_ttl(ad_2.clone(), &Duration::from_secs(300))
            .unwrap();

        assert!(store.holder.lookup(&ad_1.placement_id).unwrap().is_some());
        assert!(store.holder.lookup(&ad_2.placement_id).unwrap().is_some());

        store.invalidate_by_id(&ad_1.placement_id).unwrap();

        assert!(store.holder.lookup(&ad_1.placement_id).unwrap().is_none());
        assert!(store.holder.lookup(&ad_2.placement_id).unwrap().is_some());
    }
}
