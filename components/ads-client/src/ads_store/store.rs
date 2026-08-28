/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use crate::{
    ads_store::PlacementId,
    http_cache::{
        clock::{CacheClock, Clock},
        ByteSize,
    },
    mars::ad_response::{StorableAd, StorableAdType},
};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultKind {
    None,
    Lookup,
    Store,
    Trim,
}

pub struct AdsStoreHolder {
    conn: Mutex<Connection>,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    fault: parking_lot::Mutex<FaultKind>,
}

impl AdsStoreHolder {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(CacheClock),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    pub fn close(self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.into_inner();
        conn.close().map_err(|(_, err)| err)
    }

    #[cfg(test)]
    pub fn new_with_test_clock(conn: Connection) -> Self {
        use crate::http_cache::clock::TestClock;

        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(TestClock::new(chrono::Utc::now().timestamp())),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    #[cfg(test)]
    pub fn get_clock(&self) -> &dyn Clock {
        &*self.clock
    }

    /// Removes all entries from cache.
    pub fn clear_all(&self) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        let mut total = 0;
        total += conn.execute("DELETE FROM ads", [])?;
        Ok(total)
    }

    /// Returns total size of the cache in bytes.
    pub fn current_total_size_bytes(&self) -> SqliteResult<ByteSize> {
        let conn = self.conn.lock();
        let size_bytes_ads: u64 =
            conn.query_row("SELECT COALESCE(SUM(size_bytes),0) FROM ads", [], |row| {
                row.get(0)
            })?;
        Ok(ByteSize::b(size_bytes_ads))
    }

    pub fn lookup(&self, placement_id: &PlacementId) -> SqliteResult<Option<StorableAd>> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure"));
        }
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT placement_id, ad_type, ad_body
             FROM ads WHERE placement_id = ?1",
            params![placement_id.as_ref()],
            |row| {
                let placement_id: String = row.get(0)?;
                let ad_type: u8 = row.get(1)?;
                let ad_body: Vec<u8> = row.get(2)?;

                let placement_id = PlacementId::new(&placement_id);
                let ad_type = StorableAdType::try_from(ad_type).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Integer,
                        e.into(),
                    )
                })?;

                Ok(StorableAd {
                    placement_id,
                    ad_type,
                    ad_body,
                })
            },
        )
        .optional()
    }

    /// Upsert an object into the store.
    pub fn store_ad(&self, ad: StorableAd) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure"));
        }
        let placement_id_str: &str = ad.placement_id.as_ref();
        // placement_id char count + u8 (ad_type) + body length
        // TODO: is it actually 8 bytes? https://stackoverflow.com/questions/2761563/what-is-the-difference-between-related-sqlite-data-types-like-int-integer-smal
        let size_bytes = (placement_id_str.chars().count() + 8 + ad.ad_body.len()) as i64;
        let now = self.clock.now_epoch_seconds();

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO ads (
                stored_at,
                placement_id,
                ad_type,
                ad_body,
                size_bytes
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(placement_id) DO UPDATE SET
                stored_at=excluded.stored_at,
                ad_type=excluded.ad_type,
                ad_body=excluded.ad_body,
                size_bytes=excluded.size_bytes",
            params![
                now,
                placement_id_str,
                ad.ad_type.to_u8(),
                ad.ad_body,
                size_bytes,
            ],
        )?;
        Ok(())
    }

    pub fn invalidate_ad_by_id(&self, placement_id: &PlacementId) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM ads WHERE placement_id = ?1",
            params![&placement_id.as_ref()],
        )
    }

    pub fn trim_to_max_size(&self, max_size: &ByteSize) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Trim {
            return Err(Self::forced_fault_error("forced trim failure"));
        }
        loop {
            let total = self.current_total_size_bytes()?;
            if total.as_u64() <= max_size.as_u64() {
                break;
            }
            let conn = self.conn.lock();
            conn.execute(
                "DELETE FROM ads WHERE rowid IN (
                    SELECT rowid FROM ads ORDER BY stored_at ASC LIMIT 1
                )",
                [],
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_fault(&self, kind: FaultKind) {
        *self.fault.lock() = kind;
    }

    #[cfg(test)]
    fn forced_fault_error(msg: &str) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::InternalMalfunction,
                extended_code: 0,
            },
            Some(msg.to_string()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ads_store::connection_initializer::HttpCacheConnectionInitializer,
        mars::ad_response::{AdCallbacks, AdImage},
    };
    use sql_support::open_database;
    use url::Url;

    // Create a sample ad for tests. The body defaults to an example serialized AdImage (if body is None).
    fn create_test_raw_ad(placement_id: &str, body: Option<Vec<u8>>) -> StorableAd {
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
        StorableAd {
            placement_id: PlacementId::new(placement_id),
            ad_type: StorableAdType::Image,
            ad_body: body.unwrap_or(serde_json::to_vec(&ad).unwrap()),
        }
    }

    fn create_test_store() -> AdsStoreHolder {
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        AdsStoreHolder::new_with_test_clock(conn)
    }

    #[test]
    fn test_lookup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Lookup);

        let ad = create_test_raw_ad("mock_billboard_1", None);
        let err = store.lookup(&ad.placement_id).unwrap_err();

        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced lookup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Store);

        let ad = create_test_raw_ad("mock_billboard_1", None);

        let err = store.store_ad(ad).unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced store failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_trim_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Trim);

        let ad = create_test_raw_ad("mock_billboard_1", None);
        store.store_ad(ad).unwrap();

        let err = store.trim_to_max_size(&ByteSize::b(1)).unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced trim failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_and_retrieve_ads() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        store.store_ad(ad.clone()).unwrap();

        let retrieved = store.lookup(&ad.placement_id).unwrap().unwrap();
        assert_eq!(retrieved.ad_body, ad.ad_body);
    }

    #[test]
    fn test_max_size_eviction_ads() {
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = AdsStoreHolder::new(conn);

        for i in 0..5 {
            let large_body = vec![0u8; 300];
            let ad = create_test_raw_ad(&format!("mock_billboard_{i}"), Some(large_body));
            store.store_ad(ad.clone()).unwrap();
        }

        store.trim_to_max_size(&ByteSize::kib(1)).unwrap();

        let total_size = store.current_total_size_bytes().unwrap();
        assert!(total_size.as_u64() <= 1024);

        let first_placement_id = PlacementId::new("mock_billboard_0");
        let first_cached = store.lookup(&first_placement_id).unwrap();
        assert!(first_cached.is_none());
    }

    #[test]
    fn test_clear_all_ads() {
        let store = create_test_store();
        let ad_1 = create_test_raw_ad("mock_billboard_1", None);

        store.store_ad(ad_1.clone()).unwrap();

        let ad_2 = create_test_raw_ad("mock_billboard_2", None);
        store.store_ad(ad_2.clone()).unwrap();

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_some());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_some());

        let deleted_count = store.clear_all().unwrap();
        assert_eq!(deleted_count, 2);

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_none());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_ad_by_placement_id() {
        let store = create_test_store();

        let ad_1 = create_test_raw_ad("mock_billboard_1", None);
        let ad_2 = create_test_raw_ad("mock_billboard_2", None);

        store.store_ad(ad_1.clone()).unwrap();
        store.store_ad(ad_2.clone()).unwrap();

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_some());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_some());

        let deleted = store.invalidate_ad_by_id(&ad_1.placement_id).unwrap();
        assert_eq!(deleted, 1);

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_none());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_some());
    }
}
