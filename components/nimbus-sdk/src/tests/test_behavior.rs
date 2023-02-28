/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// cargo test --package nimbus-sdk --lib --all-features -- tests::test_behavior --nocapture

use nimbus_core::behavior::{
    EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter,
    SingleIntervalCounter,
};
use crate::error::Result;
use crate::persistence::Database;
use chrono::Utc;

#[cfg(test)]
mod event_store_tests {
    use chrono::Duration;
    use crate::behavior::DBBackedEventStore;

    use super::*;

    #[test]
    fn record_event_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter {
                data: IntervalData {
                    buckets: [0].into(),
                    bucket_count: 12,
                    starting_instant: Utc::now(),
                },
                config: IntervalConfig::new(12, Interval::Months),
            },
            SingleIntervalCounter {
                data: IntervalData {
                    buckets: [0].into(),
                    bucket_count: 28,
                    starting_instant: Utc::now(),
                },
                config: IntervalConfig::new(28, Interval::Days),
            },
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter {
                data: IntervalData {
                    buckets: [0].into(),
                    bucket_count: 12,
                    starting_instant: Utc::now(),
                },
                config: IntervalConfig::new(12, Interval::Months),
            },
            SingleIntervalCounter {
                data: IntervalData {
                    buckets: [0].into(),
                    bucket_count: 28,
                    starting_instant: Utc::now(),
                },
                config: IntervalConfig::new(28, Interval::Days),
            },
        ]);

        let mut store: Box<dyn DBBackedEventStore> = Box::new(EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]));

        let tmp_dir = tempfile::tempdir()?;
        let db = Database::new(&tmp_dir)?;

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;
        store.persist_data(&db)?;

        // Rebuild the EventStore from persisted data in order to test persistence
        let store = EventStore::try_from(&db)?;
        dbg!("From persisted data: {:?}", &store);

        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        );
        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            3
        );
        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets[1],
            0
        );
        assert_eq!(
            store
                .events
                .get(&"event-1".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets[2],
            0
        );

        assert_eq!(
            store
                .events
                .get(&"event-2".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        );
        assert_eq!(
            store
                .events
                .get(&"event-2".to_string())
                .unwrap()
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets[0],
            0
        );

        Ok(())
    }
}
