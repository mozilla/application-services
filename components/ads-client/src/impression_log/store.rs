/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::{rc::Rc, sync::Arc};

use parking_lot::Mutex;
use rusqlite::{params, types::Value, Connection, Result as SqliteResult, Row};
use sql_support::ConnExt;

use crate::clock::Clock;
use crate::impression_log::clock::ImpressionLogClock;

const SECONDS_IN_DAY: i64 = 60 * 60 * 24;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultKind {
    None,
    RecordImpression,
    CountImpressions,
    RetainImpressions,
}

pub struct ImpressionLogStore {
    conn: Mutex<Connection>,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    fault: Mutex<FaultKind>,
}

fn as_sql_values(raw_values: impl IntoIterator<Item = impl ToString>) -> Rc<Vec<Value>> {
    Rc::new(
        raw_values
            .into_iter()
            .map(|s| Value::Text(s.to_string()))
            .collect(),
    )
}

impl ImpressionLogStore {
    /// Create new store from connection
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(ImpressionLogClock),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    /// Add impression to log.
    pub fn record_impression(&self, cap_key: &str) -> SqliteResult<usize> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::RecordImpression {
            return Err(Self::forced_fault_error("forced record_impression failure"));
        }

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO impression_log (cap_key, recorded_at)
                VALUES (?1, ?2)
                ON CONFLICT (cap_key, recorded_at) DO NOTHING;",
            params![cap_key, self.clock.now_epoch_seconds()],
        )
    }

    /// Counts impressions in log.
    pub fn count_impressions(
        &self,
        cap_keys: impl IntoIterator<Item = impl ToString>,
    ) -> SqliteResult<HashMap<String, u32>> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::CountImpressions {
            return Err(Self::forced_fault_error("forced count_impressions failure"));
        }

        let conn = self.conn.lock();
        conn.query_rows_into(
            "SELECT value, COUNT(*)
                FROM impression_log
                INNER JOIN rarray(?1) ON value = cap_key
                WHERE recorded_at > ?2
                GROUP BY value;",
            params![
                as_sql_values(cap_keys),
                self.clock.now_epoch_seconds() - SECONDS_IN_DAY,
            ],
            |row: &Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
        )
    }

    /// Removes other impressions from log.
    pub fn retain_impressions(
        &self,
        cap_keys: impl IntoIterator<Item = impl ToString>,
    ) -> SqliteResult<usize> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::RetainImpressions {
            return Err(Self::forced_fault_error(
                "forced retain_impressions failure",
            ));
        }

        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM impression_log
                WHERE rowid NOT IN (
                    SELECT impression_log.rowid
                        FROM impression_log
                        INNER JOIN rarray(?1) ON value = cap_key
                );",
            params![as_sql_values(cap_keys)],
        )
    }

    #[cfg(test)]
    pub fn new_with_test_clock(conn: Connection) -> Self {
        use crate::clock::TestClock;

        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(TestClock::new(chrono::Utc::now().timestamp())),
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    #[cfg(test)]
    fn set_fault(&self, kind: FaultKind) {
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
    use crate::impression_log::connection_initializer::ImpressionLogConnectionInitializer;
    use sql_support::open_database;

    fn create_test_store() -> ImpressionLogStore {
        let initializer = ImpressionLogConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        ImpressionLogStore::new_with_test_clock(conn)
    }

    #[test]
    fn test_record_impression_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::RecordImpression);

        let err = store.record_impression("test").unwrap_err();

        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced record_impression failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_count_impressions_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::CountImpressions);

        let err = store.count_impressions(["test_cap_key"]).unwrap_err();

        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced count_impressions failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_retain_impressions_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::RetainImpressions);

        let err = store.retain_impressions(["test_cap_key"]).unwrap_err();

        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced retain_impressions failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_impression_roundtrip_simple() {
        let store = create_test_store();

        store.record_impression("test_cap_key").unwrap();

        assert_eq!(
            store.count_impressions(["test_cap_key"]).unwrap(),
            HashMap::from([("test_cap_key".into(), 1)])
        )
    }

    #[test]
    fn test_impression_roundtrip_multiple() {
        let store = create_test_store();

        store.record_impression("test_cap_key1").unwrap();
        store.record_impression("test_cap_key2").unwrap();

        assert_eq!(
            store
                .count_impressions(["test_cap_key1", "test_cap_key2"])
                .unwrap(),
            HashMap::from([("test_cap_key1".into(), 1), ("test_cap_key2".into(), 1)])
        )
    }

    #[test]
    fn test_impression_roundtrip_duplicate() {
        let store = create_test_store();

        store.record_impression("test_cap_key").unwrap();
        store.record_impression("test_cap_key").unwrap();

        assert_eq!(
            store.count_impressions(["test_cap_key"]).unwrap(),
            HashMap::from([("test_cap_key".into(), 1)])
        )
    }

    #[test]
    fn test_impression_roundtrip_over_time() {
        let store = create_test_store();

        store.record_impression("test_cap_key").unwrap();
        store.clock.advance(SECONDS_IN_DAY);
        store.record_impression("test_cap_key").unwrap();
        store.clock.advance(1);
        store.record_impression("test_cap_key").unwrap();

        assert_eq!(
            store.count_impressions(["test_cap_key"]).unwrap(),
            HashMap::from([("test_cap_key".into(), 2)])
        )
    }

    #[test]
    fn test_impression_cleanup() {
        let store = create_test_store();

        store.record_impression("test_cap_key1").unwrap();
        store.record_impression("test_cap_key2").unwrap();
        store.retain_impressions(["test_cap_key1"]).unwrap();

        assert_eq!(
            store
                .count_impressions(["test_cap_key1", "test_cap_key2"])
                .unwrap(),
            HashMap::from([("test_cap_key1".into(), 1)])
        )
    }
}
