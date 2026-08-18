use crate::mars::ad_response::{AdImage, AdSpoc, AdTile};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

// TODO: This is an intentionally naive in-memory cache implementation of the ads cache.
// It functions as a skeleton to store ads fetched in the background, and has a naive expiration mechanism.
// The subsequent vertical slice will replace this in its entirety with the http_cache sqlite database instead, with TTLs, persistent storage, etc.
const DEFAULT_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub struct AdsStore {
    ttl: Duration,

    image_ads: HashMap<String, CacheEntry<AdImage>>,
    spoc_ads: HashMap<String, CacheEntry<Vec<AdSpoc>>>,
    tile_ads: HashMap<String, CacheEntry<AdTile>>,
}

impl Default for AdsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AdsStore {
    pub fn new() -> Self {
        AdsStore::new_with_ttl(DEFAULT_TTL)
    }

    pub fn new_with_ttl(ttl: Duration) -> Self {
        AdsStore {
            ttl,
            image_ads: HashMap::new(),
            spoc_ads: HashMap::new(),
            tile_ads: HashMap::new(),
        }
    }

    pub fn store_ads<T: AdsStorable>(
        &mut self,
        ads: HashMap<String, T::StorageType>,
        timestamp: Instant,
    ) {
        T::store_ads(ads, self, timestamp);
    }

    pub fn get_stored_ads<'a, T: AdsStorable>(
        &'a self,
        placement: &str,
    ) -> Option<&'a T::StorageType> {
        T::fetch_stored_ads(self, placement)
    }
}

pub trait AdsStorable: Sized {
    // The ad(s) to store (eg: this may be a single ad, or an array of ads)
    type StorageType;

    fn store_ads(
        ads: HashMap<String, Self::StorageType>,
        ads_cache: &mut AdsStore,
        timestamp: Instant,
    );
    fn fetch_stored_ads<'a>(ads_cache: &'a AdsStore, id: &str) -> Option<&'a Self::StorageType>;
}

impl AdsStorable for AdImage {
    type StorageType = AdImage;
    fn store_ads(ads: HashMap<String, AdImage>, ads_cache: &mut AdsStore, timestamp: Instant) {
        ads_cache.image_ads.extend(
            ads.into_iter()
                .map(|(key, ad)| (key, CacheEntry::new(ad, timestamp))),
        );
        ads_cache
            .image_ads
            .retain(|_, x| !x.is_expired(ads_cache.ttl));
    }

    fn fetch_stored_ads<'a>(ads_cache: &'a AdsStore, id: &str) -> Option<&'a AdImage> {
        ads_cache.image_ads.get(id).map(|ads| ads.get_value())
    }
}

impl AdsStorable for AdSpoc {
    type StorageType = Vec<AdSpoc>;
    fn store_ads(ads: HashMap<String, Vec<AdSpoc>>, ads_cache: &mut AdsStore, timestamp: Instant) {
        ads_cache.spoc_ads.extend(
            ads.into_iter()
                .map(|(key, ad)| (key, CacheEntry::new(ad, timestamp))),
        );
        ads_cache
            .spoc_ads
            .retain(|_, x| !x.is_expired(ads_cache.ttl));
    }
    fn fetch_stored_ads<'a>(ads_cache: &'a AdsStore, id: &str) -> Option<&'a Vec<AdSpoc>> {
        ads_cache.spoc_ads.get(id).map(|ads| ads.get_value())
    }
}

impl AdsStorable for AdTile {
    type StorageType = AdTile;
    fn store_ads(ads: HashMap<String, AdTile>, ads_cache: &mut AdsStore, timestamp: Instant) {
        ads_cache.tile_ads.extend(
            ads.into_iter()
                .map(|(key, ad)| (key, CacheEntry::new(ad, timestamp))),
        );
        ads_cache
            .tile_ads
            .retain(|_, x| !x.is_expired(ads_cache.ttl));
    }
    fn fetch_stored_ads<'a>(ads_cache: &'a AdsStore, id: &str) -> Option<&'a AdTile> {
        ads_cache.tile_ads.get(id).map(|ads| ads.get_value())
    }
}

#[derive(Debug)]
struct CacheEntry<T> {
    inserted_at: Instant,
    value: T,
}

impl<T> CacheEntry<T> {
    fn new(value: T, instant: Instant) -> CacheEntry<T> {
        CacheEntry {
            inserted_at: instant,
            value,
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.inserted_at.elapsed() >= ttl
    }

    fn get_value(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        time::{Duration, Instant},
    };

    use crate::{
        ads_store::AdsStore,
        mars::ad_response::{AdImage, AdSpoc, AdTile},
        test_utils,
    };

    #[test]
    fn test_store_image_ad() {
        let five_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(5))
            .expect("Could not create `Instant` for 5 minutes ago");
        let one_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(1))
            .expect("Could not create `Instant` for 1 minute ago");
        let mut ads_store = AdsStore::new_with_ttl(Duration::from_mins(3));

        let demo_ads = test_utils::get_example_happy_image_response().data;
        let first_key = demo_ads
            .iter()
            .next()
            .expect("No test data in `get_example_happy_image_response`")
            .0;
        // TODO: Remove this bit.
        let demo_ads: HashMap<String, AdImage> = demo_ads
            .clone()
            .into_iter()
            .filter_map(|(k, v)| Some((k, v.into_iter().next()?)))
            .collect();

        ads_store.store_ads::<AdImage>(demo_ads.clone(), five_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdImage>(first_key).is_none(),
            "Old data past TTL date must not be returned."
        );

        ads_store.store_ads::<AdImage>(demo_ads, one_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdImage>(first_key).is_some(),
            "Could not fetch fresh ad from ads store."
        );
    }

    #[test]
    fn test_store_spocs_ad() {
        let five_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(5))
            .expect("Could not create `Instant` for 5 minutes ago");
        let one_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(1))
            .expect("Could not create `Instant` for 1 minute ago");
        let mut ads_store = AdsStore::new_with_ttl(Duration::from_mins(3));

        let demo_ads = test_utils::get_example_happy_spoc_response().data;
        let first_key = demo_ads
            .iter()
            .next()
            .expect("No test data in `get_example_happy_spoc_response`")
            .0;
        // TODO: Remove this bit.
        // let demo_ads: HashMap<String, Vec<AdSpoc>>  = demo_ads.clone().into_iter().filter_map(|(k,v)| Some((k, v.into_iter().next()?))).collect();

        ads_store.store_ads::<AdSpoc>(demo_ads.clone(), five_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdSpoc>(first_key).is_none(),
            "Old data past TTL date must not be returned."
        );

        ads_store.store_ads::<AdSpoc>(demo_ads.clone(), one_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdSpoc>(first_key).is_some(),
            "Could not fetch fresh ad from ads store."
        );
    }

    #[test]

    fn test_store_tiles_ad() {
        let five_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(5))
            .expect("Could not create `Instant` for 5 minutes ago");
        let one_min_ago = Instant::now()
            .checked_sub(Duration::from_mins(1))
            .expect("Could not create `Instant` for 1 minute ago");
        let mut ads_store = AdsStore::new_with_ttl(Duration::from_mins(3));

        let demo_ads = test_utils::get_example_happy_uatile_response().data;
        let first_key = demo_ads
            .iter()
            .next()
            .expect("No test data in `get_example_happy_uatile_response`")
            .0;
        // TODO: Remove this bit.
        let demo_ads: HashMap<String, AdTile> = demo_ads
            .clone()
            .into_iter()
            .filter_map(|(k, v)| Some((k, v.into_iter().next()?)))
            .collect();

        ads_store.store_ads::<AdTile>(demo_ads.clone(), five_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdTile>(first_key).is_none(),
            "Old data past TTL date must not be returned."
        );

        ads_store.store_ads::<AdTile>(demo_ads, one_min_ago);
        assert!(
            ads_store.get_stored_ads::<AdTile>(first_key).is_some(),
            "Could not fetch fresh ad from ads store."
        );
    }
}
