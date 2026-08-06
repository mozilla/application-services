use crate::mars::ad_response::{AdImage, AdSpoc, AdTile};
use std::{collections::HashMap, time::Duration};

// TODO: This is an intentionally naive in-memory cache implementation of the ads cache.
// It functions as a skeleton to store ads fetched in the background, and has a naive expiration mechanism.
// The subsequent vertical slice will replace this in its entirety with the http_cache sqlite database instead, with TTLs, persistent storage, etc.
const DEFAULT_TTL: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub struct AdsCache {
    image_ads: HashMap<String, (u64, AdImage)>,
    spoc_ads: HashMap<String, (u64, Vec<AdSpoc>)>,
    tile_ads: HashMap<String, (u64, AdTile)>,
}

impl Default for AdsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AdsCache {
    pub fn new() -> Self {
        AdsCache {
            image_ads: HashMap::new(),
            spoc_ads: HashMap::new(),
            tile_ads: HashMap::new(),
        }
    }

    pub fn cache_ads<T: AdsCacheable>(
        &mut self,
        ads: HashMap<String, T::StorageType>,
        timestamp: u64,
    ) {
        T::cache_ads(ads, self, timestamp);
    }

    pub fn get_cached_ads<'a, T: AdsCacheable>(
        &'a self,
        placement: &str,
    ) -> Option<&'a T::StorageType> {
        T::fetch_cached_ads(self, placement)
    }
}

pub trait AdsCacheable: Sized {
    // The cached ad(s) to store (eg: this may be a single ad, or an array of ads)
    type StorageType;

    fn cache_ads(ads: HashMap<String, Self::StorageType>, ads_cache: &mut AdsCache, timestamp: u64);
    fn fetch_cached_ads<'a>(ads_cache: &'a AdsCache, id: &str) -> Option<&'a Self::StorageType>;
}

impl AdsCacheable for AdImage {
    type StorageType = AdImage;
    fn cache_ads(ads: HashMap<String, AdImage>, ads_cache: &mut AdsCache, timestamp: u64) {
        ads_cache
            .image_ads
            .extend(ads.into_iter().map(|(key, ad)| (key, (timestamp, ad))));
        ads_cache
            .image_ads
            .retain(|_, (x, _)| *x - timestamp < DEFAULT_TTL.as_secs());
    }

    fn fetch_cached_ads<'a>(ads_cache: &'a AdsCache, id: &str) -> Option<&'a AdImage> {
        ads_cache.image_ads.get(id).map(|(_, ads)| ads)
    }
}

impl AdsCacheable for AdSpoc {
    type StorageType = Vec<AdSpoc>;
    fn cache_ads(ads: HashMap<String, Vec<AdSpoc>>, ads_cache: &mut AdsCache, timestamp: u64) {
        ads_cache
            .spoc_ads
            .extend(ads.into_iter().map(|(key, ad)| (key, (timestamp, ad))));
        ads_cache
            .spoc_ads
            .retain(|_, (x, _)| *x - timestamp < DEFAULT_TTL.as_secs());
    }
    fn fetch_cached_ads<'a>(ads_cache: &'a AdsCache, id: &str) -> Option<&'a Vec<AdSpoc>> {
        ads_cache.spoc_ads.get(id).map(|(_, ads)| ads)
    }
}

impl AdsCacheable for AdTile {
    type StorageType = AdTile;
    fn cache_ads(ads: HashMap<String, AdTile>, ads_cache: &mut AdsCache, timestamp: u64) {
        ads_cache
            .tile_ads
            .extend(ads.into_iter().map(|(key, ad)| (key, (timestamp, ad))));
        ads_cache
            .tile_ads
            .retain(|_, (x, _)| *x - timestamp < DEFAULT_TTL.as_secs());
    }
    fn fetch_cached_ads<'a>(ads_cache: &'a AdsCache, id: &str) -> Option<&'a AdTile> {
        ads_cache.tile_ads.get(id).map(|(_, ads)| ads)
    }
}
