/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{sync::Arc, time::Duration};

use crate::{
    ads_store::PlacementId,
    http_cache::{
        clock::{CacheClock, Clock},
        ByteSize,
    },
    mars::ad_response::{RawAd, RawAdType},
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
    Cleanup,
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

    /// Removes all entries from the store whose expires_at is at or before the current time.
    pub fn delete_expired_entries(&self) -> SqliteResult<usize> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Cleanup {
            return Err(Self::forced_fault_error("forced cleanup failure"));
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let mut total = 0;
        total += tx.execute(
            "DELETE FROM ads WHERE expires_at <= ?1",
            params![self.clock.now_epoch_seconds()],
        )?;
        tx.commit()?;
        Ok(total)
    }
    /// Lookup is agnostic to expiration. If it exists in the store, it will return the result.
    pub fn lookup(&self, placement_id: &PlacementId) -> SqliteResult<Option<RawAd>> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure"));
        }
        let conn = self.conn.lock();
        // TODO: Should we use body or explicit fields?
        conn.query_row(
            "SELECT placement_id, placement_type, placement_body
             FROM ads WHERE placement_id = ?1",
            params![placement_id.as_ref()],
            |row| {
                let placement_id: String = row.get(0)?;
                let placement_type: u8 = row.get(1)?;
                let placement_body: Vec<u8> = row.get(2)?;

                let placement_id = PlacementId::new(&placement_id);
                let placement_type = RawAdType::try_from(placement_type).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Integer,
                        e.into(),
                    )
                })?;

                Ok(RawAd {
                    placement_id,
                    placement_type,
                    placement_body,
                })
            },
        )
        .optional()
    }

    /// Upsert an object into the store with an expires_at defined by the given ttl_seconds.
    /// Calling this method will always store an object regardless of headers or policy.
    /// Logic to determine the correct ttl or cache/no-cache should happen before calling this.
    /// TODO: maybe this should take a raw ad? maybe no need for raw ad at all?
    pub fn store_with_ttl(
        &self,
        placement_id: &PlacementId,
        placement_type: RawAdType,
        placement_body: Vec<u8>,
        ttl: &Duration,
    ) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure"));
        }
        let placement_id_str : &str = placement_id.as_ref();
        // placement_id char count + u8 (placement_type) + body length
        // TODO: is it actually 8 bytes? https://stackoverflow.com/questions/2761563/what-is-the-difference-between-related-sqlite-data-types-like-int-integer-smal
        let size_bytes = (placement_id_str.chars().count() + 8 + placement_body.len()) as i64;
        let now = self.clock.now_epoch_seconds();
        let ttl_seconds = ttl.as_secs();
        let expires_at = now + ttl_seconds as i64;

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO ads (
                cached_at,
                expires_at,
                placement_id,
                placement_type,
                placement_body,
                size_bytes,
                ttl_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(placement_id) DO UPDATE SET
                cached_at=excluded.cached_at,
                expires_at=excluded.expires_at,
                placement_type=excluded.placement_type,
                placement_body=excluded.placement_body,
                size_bytes=excluded.size_bytes,
                ttl_seconds=excluded.ttl_seconds",
            params![
                now,
                expires_at,
                placement_id_str,
                placement_type.to_u8(),
                placement_body,
                size_bytes,
                ttl_seconds as i64,
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
                    SELECT rowid FROM ads ORDER BY cached_at ASC LIMIT 1
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
    use std::time::Duration;
    use url::Url;

    fn fetch_timestamps(store: &AdsStoreHolder, placement_id: &PlacementId) -> (i64, i64, i64) {
        let conn = store.conn.lock();
        conn.query_row(
            "SELECT
                    cached_at,
                    expires_at,
                    COALESCE(ttl_seconds, -1)
            FROM ads WHERE placement_id = ?1",
            rusqlite::params![&placement_id.as_ref()],
            |row| {
                let cached_at: i64 = row.get(0)?;
                let expires_at: i64 = row.get(1)?;
                let ttl: i64 = row.get(2)?;
                Ok((cached_at, expires_at, ttl))
            },
        )
        .expect("row should exist")
    }

    // Create a sample ad for tests. The body defaults to an example serialized AdImage (if body is None).
    fn create_test_raw_ad(placement_id: &str, body: Option<Vec<u8>>) -> RawAd {
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
        RawAd {
            placement_id: PlacementId::new(placement_id),
            placement_type: RawAdType::Image,
            placement_body: body.unwrap_or(serde_json::to_vec(&ad).unwrap()),
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

        let err = store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap_err();
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
        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();

        let err = store.trim_to_max_size(&ByteSize::b(1)).unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced trim failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_cleanup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Cleanup);

        let err = store.delete_expired_entries().unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced cleanup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_ads_with_ttl_sets_fields_consistently() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        let ttl = Duration::from_secs(5);
        store
            .store_with_ttl(&ad.placement_id, ad.placement_type, ad.placement_body, &ttl)
            .unwrap();

        let (cached_at, expires_at, ttl_seconds) = fetch_timestamps(&store, &ad.placement_id);
        assert_eq!(ttl_seconds, ttl.as_secs() as i64);
        let diff = expires_at - cached_at;
        let ttl_seconds = ttl.as_secs();
        assert!(
            (diff == ttl_seconds as i64)
                || (diff == ttl_seconds as i64 - 1)
                || (diff == ttl_seconds as i64 + 1),
            "unexpected expires_at diff: got {diff}, want ~{ttl_seconds}"
        );
    }

    #[test]
    fn test_upsert_ads_refreshes_ttl_and_expiry() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body.clone(),
                &Duration::from_secs(300),
            )
            .unwrap();
        let (c1, e1, t1) = fetch_timestamps(&store, &ad.placement_id);
        assert_eq!(t1, 300);

        store.get_clock().advance(3);

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(1),
            )
            .unwrap();
        let (c2, e2, t2) = fetch_timestamps(&store, &ad.placement_id);
        assert_eq!(t2, 1);
        assert!(c2 > c1);
        assert!(e2 < e1, "expires_at should move earlier when TTL shrinks");
    }

    #[test]
    fn test_delete_expired_removes_only_expired_ads() {
        let store = create_test_store();

        let ad_exp = create_test_raw_ad("mock_billboard_1", None);
        let ad_fresh = create_test_raw_ad("mock_billboard_2", None);

        store
            .store_with_ttl(
                &ad_exp.placement_id,
                ad_exp.placement_type,
                ad_exp.placement_body,
                &Duration::from_secs(1),
            )
            .unwrap();
        store
            .store_with_ttl(
                &ad_fresh.placement_id,
                ad_fresh.placement_type,
                ad_fresh.placement_body,
                &Duration::from_secs(10),
            )
            .unwrap();

        assert!(store.lookup(&ad_exp.placement_id).unwrap().is_some());
        assert!(store.lookup(&ad_fresh.placement_id).unwrap().is_some());

        store.clock.advance(2);
        let removed = store.delete_expired_entries().unwrap();
        assert!(
            removed >= 1,
            "expected at least one expired row to be deleted"
        );

        assert!(store.lookup(&ad_exp.placement_id).unwrap().is_none());
        assert!(store.lookup(&ad_fresh.placement_id).unwrap().is_some());
    }

    #[test]
    fn test_lookups_is_expired_agnostic() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(1),
            )
            .unwrap();
        store.clock.advance(2);
        assert!(store.lookup(&ad.placement_id).unwrap().is_some());

        store.delete_expired_entries().unwrap();
        assert!(store.lookup(&ad.placement_id).unwrap().is_none());
    }

    #[test]
    fn test_zero_ttl_expires_ads_immediately_after_tick() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(0),
            )
            .unwrap();
        assert!(store.lookup(&ad.placement_id).unwrap().is_some());

        store.clock.advance(2);
        let removed = store.delete_expired_entries().unwrap();
        assert!(removed >= 1);
        assert!(store.lookup(&ad.placement_id).unwrap().is_none());
    }

    #[test]
    fn test_store_and_retrieve_ads() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", None);

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body.clone(),
                &Duration::from_secs(300),
            )
            .unwrap();

        let retrieved = store.lookup(&ad.placement_id).unwrap().unwrap();
        assert_eq!(retrieved.placement_body, ad.placement_body);
    }

    #[test]
    fn test_ttl_expiration_ads() {
        let store = create_test_store();
        let ad = create_test_raw_ad("mock_billboard_1", Some(b"test response".to_vec()));

        store
            .store_with_ttl(
                &ad.placement_id,
                ad.placement_type,
                ad.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();

        let retrieved = store.lookup(&ad.placement_id).unwrap().unwrap();
        assert_eq!(retrieved.placement_body, b"test response");

        store.clock.advance(2);

        let retrieved_after_expiry = store.lookup(&ad.placement_id).unwrap();
        assert!(retrieved_after_expiry.is_some());
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
            store
                .store_with_ttl(
                    &ad.placement_id,
                    ad.placement_type,
                    ad.placement_body,
                    &Duration::from_secs(300),
                )
                .unwrap();
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

        store
            .store_with_ttl(
                &ad_1.placement_id,
                ad_1.placement_type,
                ad_1.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();

        let ad_2 = create_test_raw_ad("mock_billboard_2", None);
        store
            .store_with_ttl(
                &ad_2.placement_id,
                ad_2.placement_type,
                ad_2.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();

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

        store
            .store_with_ttl(
                &ad_1.placement_id,
                ad_1.placement_type,
                ad_1.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();
        store
            .store_with_ttl(
                &ad_2.placement_id,
                ad_2.placement_type,
                ad_2.placement_body,
                &Duration::from_secs(300),
            )
            .unwrap();

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_some());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_some());

        let deleted = store.invalidate_ad_by_id(&ad_1.placement_id).unwrap();
        assert_eq!(deleted, 1);

        assert!(store.lookup(&ad_1.placement_id).unwrap().is_none());
        assert!(store.lookup(&ad_2.placement_id).unwrap().is_some());
    }
}
