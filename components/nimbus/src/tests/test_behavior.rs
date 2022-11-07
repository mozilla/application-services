/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// cargo test --package nimbus-sdk --lib --all-features -- tests::test_behavior --nocapture

use crate::behavior::{
    EventQueryType, EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter,
    SingleIntervalCounter,
};
use crate::error::Result;
use crate::persistence::Database;
use chrono::{DateTime, Utc};
use std::collections::VecDeque;

#[cfg(test)]
mod interval_tests {
    use super::*;

    #[test]
    fn increment_num_rotations_minutes() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-10-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-01T10:04:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Minutes.num_rotations(d1, d2)?;

        assert_eq!(num, 4);

        let d1 = DateTime::parse_from_rfc3339("2022-10-01T10:55:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-01T11:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Minutes.num_rotations(d1, d2)?;

        assert_eq!(num, 20);

        let d1 = DateTime::parse_from_rfc3339("2022-09-30T10:50:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-01T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Minutes.num_rotations(d1, d2)?;

        assert_eq!(num, 1360);
        Ok(())
    }

    #[test]
    fn increment_num_rotations_hours() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-10-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-01T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Hours.num_rotations(d1, d2)?;

        assert_eq!(num, 4);

        let d1 = DateTime::parse_from_rfc3339("2022-10-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-02T06:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Hours.num_rotations(d1, d2)?;

        assert_eq!(num, 20);
        Ok(())
    }

    #[test]
    fn increment_num_rotations_days() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-09-28T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-05T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Days.num_rotations(d1, d2)?;

        assert_eq!(num, 7);
        Ok(())
    }

    #[test]
    fn increment_num_rotations_weeks() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-09-28T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-08T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Weeks.num_rotations(d1, d2)?;

        assert_eq!(num, 1);
        Ok(())
    }

    #[test]
    fn increment_num_rotations_months() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-10-08T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Months.num_rotations(d1, d2)?;

        assert_eq!(num, 4);
        Ok(())
    }

    #[test]
    fn increment_num_rotations_years() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2012-06-10T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-04-08T14:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let num = Interval::Years.num_rotations(d1, d2)?;

        assert_eq!(num, 9);
        Ok(())
    }
}

#[cfg(test)]
mod interval_data_tests {
    use super::*;

    #[test]
    fn increment_works_if_no_buckets_present() -> Result<()> {
        let mut interval = IntervalData {
            buckets: VecDeque::new(),
            bucket_count: 7,
            starting_instant: Utc::now(),
        };
        let result = interval.increment();

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn increment_increments_front_bucket_if_it_exists() -> Result<()> {
        let mut interval = IntervalData::new(7);
        interval.increment().ok();

        assert_eq!(interval.buckets[0], 1);
        Ok(())
    }

    #[test]
    fn rotate_adds_buckets_for_each_rotation() -> Result<()> {
        let mut interval = IntervalData::new(7);
        interval.rotate(3).ok();

        assert_eq!(interval.buckets.len(), 4);
        Ok(())
    }

    #[test]
    fn rotate_removes_buckets_when_max_is_reached() -> Result<()> {
        let mut interval = IntervalData::new(3);
        interval.increment().ok();
        interval.rotate(2).ok();
        interval.increment().ok();
        interval.rotate(1).ok();
        interval.increment().ok();
        interval.increment().ok();

        assert_eq!(interval.buckets.len(), 3);
        assert_eq!(interval.buckets[0], 2);
        assert_eq!(interval.buckets[1], 1);
        assert_eq!(interval.buckets[2], 0);
        Ok(())
    }

    #[test]
    fn rotate_handles_large_rotation() -> Result<()> {
        let mut interval = IntervalData::new(3);
        interval.rotate(10).ok();

        assert_eq!(interval.buckets.len(), 3);
        assert_eq!(interval.buckets[0], 0);
        assert_eq!(interval.buckets[1], 0);
        assert_eq!(interval.buckets[2], 0);
        Ok(())
    }
}

#[cfg(test)]
mod single_interval_counter_tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn test_increment() -> Result<()> {
        let mut counter = SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days));
        counter.increment().ok();

        assert_eq!(counter.data.buckets[0], 1);
        Ok(())
    }

    #[test]
    fn test_advance_do_not_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days));
        let date = Utc::now();
        counter.maybe_advance(date).ok();

        assert_eq!(counter.data.buckets.len(), 1);
        Ok(())
    }

    #[test]
    fn test_advance_do_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days));
        let date = Utc::now() + Duration::days(1);
        counter.maybe_advance(date).ok();

        assert_eq!(counter.data.buckets.len(), 2);
        Ok(())
    }
}

#[cfg(test)]
mod multi_interval_counter_tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn test_increment_many() -> Result<()> {
        let mut counter = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);
        counter.increment().ok();

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            counter.intervals.get(&Interval::Days).unwrap().data.buckets[0],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_do_not_advance() -> Result<()> {
        let mut counter = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);
        let date = Utc::now();
        counter.maybe_advance(date).ok();

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        );
        assert_eq!(
            counter
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_advance_some() -> Result<()> {
        let mut counter = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);
        let date = Utc::now() + Duration::days(1);
        counter.maybe_advance(date).ok();

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        );
        assert_eq!(
            counter
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            2
        );
        Ok(())
    }
}

#[cfg(test)]
mod event_store_tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn record_event_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

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

    #[test]
    fn query_sum_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                0,
                EventQueryType::Sum
            )?,
            3.0
        );
        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                0,
                0,
                EventQueryType::Sum
            )?,
            0.0
        );
        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                7,
                EventQueryType::Sum
            )?,
            0.0
        );

        Ok(())
    }

    #[test]
    fn query_count_non_zero_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                0,
                EventQueryType::CountNonZero
            )?,
            2.0
        );
        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                2,
                EventQueryType::CountNonZero
            )?,
            0.0
        );

        Ok(())
    }

    #[test]
    fn query_average_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                0,
                EventQueryType::AveragePerInterval
            )?,
            0.42857142857142855
        );
        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                2,
                0,
                EventQueryType::AveragePerInterval
            )?,
            1.5
        );

        Ok(())
    }

    #[test]
    fn query_average_per_non_zero_interval_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let counter2 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;
        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                0,
                EventQueryType::AveragePerNonZeroInterval
            )?,
            1.5
        );
        assert_eq!(
            store.query(
                "event-1".to_string(),
                Interval::Days,
                7,
                2,
                EventQueryType::AveragePerNonZeroInterval
            )?,
            0.0
        );

        Ok(())
    }
}
