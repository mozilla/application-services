pub mod builder;
pub mod connection_initializer;
pub mod store;

use crate::{
    ads::PlacementId,
    ads_store::{builder::AdsStoreBuilder, store::AdsStoreHolder},
    common::bytesize::ByteSize,
};
use std::path::Path;

pub struct AdsStore {
    holder: AdsStoreHolder,
    #[allow(dead_code)]
    max_size: ByteSize,
    is_memory: bool,
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

    pub fn is_memory(&self) -> bool {
        self.is_memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ffi::telemetry::MozAdsTelemetryWrapper, test_utils::get_example_happy_image_ads};

    #[test]
    fn test_ads_store_creation() {
        // Test that AdsStore can be created successfully with test config
        let store: Result<AdsStore, _> =
            AdsStore::builder("test_store.db").build(MozAdsTelemetryWrapper::noop());
        assert!(store.is_ok());
    }

    #[test]
    fn test_clear_store() {
        let store: AdsStore = AdsStore::builder("test_clear.db")
            .build(MozAdsTelemetryWrapper::noop())
            .unwrap();

        let (placement_id, ad) = get_example_happy_image_ads("mock_billboard_1");
        store.holder.store_ad(&placement_id, ad.clone()).unwrap();

        // Verify it's cached
        let retrieved = store.holder.lookup(&placement_id).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        store.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = store.holder.lookup(&placement_id).unwrap();
        assert!(retrieved_after_clear.is_none());
    }

    #[test]
    fn test_invalidate_by_id() {
        let store: AdsStore = AdsStore::builder("test_invalidate.db")
            .build(MozAdsTelemetryWrapper::noop())
            .unwrap();

        let (placement_1, ad_1) = get_example_happy_image_ads("mock_billboard_1");
        let (placement_2, ad_2) = get_example_happy_image_ads("mock_billboard_2");
        store.holder.store_ad(&placement_1, ad_1).unwrap();
        store.holder.store_ad(&placement_2, ad_2).unwrap();

        assert!(store.holder.lookup(&placement_1).unwrap().is_some());
        assert!(store.holder.lookup(&placement_2).unwrap().is_some());

        store.invalidate_by_id(&placement_1).unwrap();

        assert!(store.holder.lookup(&placement_1).unwrap().is_none());
        assert!(store.holder.lookup(&placement_2).unwrap().is_some());
    }
}
