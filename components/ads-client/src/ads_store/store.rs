/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::ads_store::StorableAd;
use crate::database::bytesize::ByteSize;
use crate::database::clock::Clock;
use crate::mars::error::FetchAdsError;
use crate::{ads_store::PlacementId, database::clock::CacheClock};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use std::sync::Arc;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultKind {
    Lookup,
    None,
    Store,
    Trim,
}

pub struct AdsStoreHolder {
    clock: Arc<dyn Clock>,
    conn: Mutex<Connection>,
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
        use crate::database::clock::TestClock;

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

    pub fn lookup(&self, placement_id: &PlacementId) -> Result<Option<StorableAd>, FetchAdsError> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure").into());
        }
        let conn = self.conn.lock();
        let res = conn
            .query_row(
                "SELECT placement_id, ad_body
             FROM ads WHERE placement_id = ?1",
                params![placement_id.as_ref()],
                |row| {
                    let ad_body: Vec<u8> = row.get(1)?;
                    Ok(ad_body)
                },
            )
            .optional()?;
        Ok(res.map(|x| serde_json::from_slice(&x)).transpose()?)
    }

    /// Upsert an object into the store.
    pub fn store_ad(
        &self,
        placement_id: &PlacementId,
        ad: StorableAd,
    ) -> Result<(), FetchAdsError> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure").into());
        }
        let placement_id_str: &str = placement_id.as_ref();
        let ad_body = serde_json::to_vec(&ad)?;
        let size_bytes = ad_body.len() as i64;
        let now = self.clock.now_epoch_seconds();

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO ads (
                stored_at,
                placement_id,
                ad_body,
                size_bytes
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(placement_id) DO UPDATE SET
                stored_at=excluded.stored_at,
                ad_body=excluded.ad_body,
                size_bytes=excluded.size_bytes",
            params![now, placement_id_str, ad_body, size_bytes,],
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
        ads_store::connection_initializer::AdsStoreConnectionInitializer,
        mars::ad_response::{AdCallbacks, AdImage},
    };
    use sql_support::open_database;
    use url::Url;

    // Create a sample ad for tests. The body defaults to an example serialized AdImage (if body is None).
    fn create_test_raw_ad(placement_id: &str) -> (PlacementId, StorableAd) {
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
        (PlacementId::new(placement_id), StorableAd::Image(ad))
    }

    fn create_test_store() -> AdsStoreHolder {
        let initializer = AdsStoreConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        AdsStoreHolder::new_with_test_clock(conn)
    }

    #[test]
    fn test_lookup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Lookup);

        let (placement, _) = create_test_raw_ad("mock_billboard_1");
        let err = store.lookup(&placement).unwrap_err();

        match err {
            FetchAdsError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                assert!(msg.contains("forced lookup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Store);

        let (placement, ad) = create_test_raw_ad("mock_billboard_1");

        let err = store.store_ad(&placement, ad).unwrap_err();
        match err {
            FetchAdsError::Sqlite(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                assert!(msg.contains("forced store failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_trim_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Trim);

        let (placement, ad) = create_test_raw_ad("mock_billboard_1");
        store.store_ad(&placement, ad).unwrap();

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
        let (placement, ad) = create_test_raw_ad("mock_billboard_1");

        store.store_ad(&placement, ad.clone()).unwrap();

        let retrieved = store.lookup(&placement).unwrap().unwrap();
        assert_eq!(retrieved, ad);
    }

    #[test]
    fn test_max_size_eviction_ads() {
        let initializer = AdsStoreConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = AdsStoreHolder::new(conn);

        for i in 0..10 {
            let (placement_id, ad) = create_test_raw_ad(&format!("mock_billboard_{i}"));
            store.store_ad(&placement_id, ad.clone()).unwrap();
        }

        let total_size = store.current_total_size_bytes().unwrap();
        assert!(total_size.as_u64() >= 1024);

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
        let (placement_1, ad_1) = create_test_raw_ad("mock_billboard_1");

        store.store_ad(&placement_1, ad_1.clone()).unwrap();

        let (placement_2, ad_2) = create_test_raw_ad("mock_billboard_2");
        store.store_ad(&placement_2, ad_2.clone()).unwrap();

        assert!(store.lookup(&placement_1).unwrap().is_some());
        assert!(store.lookup(&placement_2).unwrap().is_some());

        let deleted_count = store.clear_all().unwrap();
        assert_eq!(deleted_count, 2);

        assert!(store.lookup(&placement_1).unwrap().is_none());
        assert!(store.lookup(&placement_2).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_ad_by_placement_id() {
        let store = create_test_store();

        let (placement_1, ad_1) = create_test_raw_ad("mock_billboard_1");
        let (placement_2, ad_2) = create_test_raw_ad("mock_billboard_2");

        store.store_ad(&placement_1, ad_1.clone()).unwrap();
        store.store_ad(&placement_2, ad_2.clone()).unwrap();

        assert!(store.lookup(&placement_1).unwrap().is_some());
        assert!(store.lookup(&placement_2).unwrap().is_some());

        let deleted = store.invalidate_ad_by_id(&placement_1).unwrap();
        assert_eq!(deleted, 1);

        assert!(store.lookup(&placement_1).unwrap().is_none());
        assert!(store.lookup(&placement_2).unwrap().is_some());
    }
}
