/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// cargo test --package nimbus-sdk --lib --all-features -- tests::test_behavior --nocapture

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::stateful::behavior::{
    EventQueryType, EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter,
    SingleIntervalCounter,
};
use crate::stateful::persistence::Database;

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
        let result = interval.increment(1);

        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn increment_increments_front_bucket_if_it_exists() -> Result<()> {
        let mut interval = IntervalData::new(7);
        interval.increment(1).ok();

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
        interval.increment(1).ok();
        interval.rotate(2).ok();
        interval.increment(1).ok();
        interval.rotate(1).ok();
        interval.increment(1).ok();
        interval.increment(1).ok();

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
        counter.increment(1).ok();

        assert_eq!(counter.data.buckets[0], 1);
        Ok(())
    }

    #[test]
    fn test_advance_do_not_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter {
            data: IntervalData {
                buckets: [0].into(),
                bucket_count: 7,
                starting_instant: Utc::now(),
            },
            config: IntervalConfig::new(7, Interval::Days),
        };
        let date = Utc::now();
        counter.maybe_advance(date).ok();

        assert_eq!(counter.data.buckets.len(), 1);
        Ok(())
    }

    #[test]
    fn test_advance_do_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter {
            data: IntervalData {
                buckets: [0].into(),
                bucket_count: 7,
                starting_instant: Utc::now(),
            },
            config: IntervalConfig::new(7, Interval::Days),
        };
        let date = Utc::now() + Duration::days(1);
        counter.maybe_advance(date).ok();

        assert_eq!(counter.data.buckets.len(), 2);
        Ok(())
    }

    #[test]
    fn test_maybe_advance_updates_time_in_minutes_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-10T08:00:59Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2022-06-10T08:01:58Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2022-06-10T08:02:57Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Minutes),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_maybe_advance_updates_time_in_hours_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-10T08:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2022-06-10T09:58:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2022-06-10T10:57:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Hours),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_maybe_advance_updates_time_in_days_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-11T07:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2022-06-12T07:58:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2022-06-13T07:57:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Days),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_maybe_advance_updates_time_in_weeks_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-17T07:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2022-06-25T07:58:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2022-07-01T07:57:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Weeks),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_maybe_advance_updates_time_in_months_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-07-07T07:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2022-08-02T07:58:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2022-08-30T07:57:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Months),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_maybe_advance_updates_time_in_years_correctly() {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2023-06-10T07:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d3 = DateTime::parse_from_rfc3339("2024-06-09T07:58:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d4 = DateTime::parse_from_rfc3339("2025-06-09T07:57:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: d1,
            },
            config: IntervalConfig::new(7, Interval::Years),
        };
        counter.data.buckets.push_front(0);

        counter.maybe_advance(d2).unwrap();
        counter.maybe_advance(d3).unwrap();
        counter.maybe_advance(d4).unwrap();

        assert_eq!(counter.data.buckets.len(), 3);
    }

    #[test]
    fn test_increment_events_in_the_past() -> Result<()> {
        let t0: DateTime<Utc> = DateTime::parse_from_rfc3339("2023-03-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let mut counter: SingleIntervalCounter = SingleIntervalCounter {
            data: IntervalData {
                bucket_count: 7,
                buckets: VecDeque::with_capacity(7),
                starting_instant: t0,
            },
            config: IntervalConfig::new(7, Interval::Days),
        };

        // t0 + 12h
        let then = DateTime::parse_from_rfc3339("2023-03-10T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(Interval::Days.num_rotations(then, t0)?, 0);
        counter.increment_then(then, 1)?;
        assert_eq!(counter.data.buckets[0], 1u64);

        // t0 - 12h
        let then = DateTime::parse_from_rfc3339("2023-03-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(Interval::Days.num_rotations(then, t0)?, 0);
        counter.increment_then(then, 2)?;
        assert_eq!(counter.data.buckets[1], 2u64);

        // t0 - 1d12h
        let then = DateTime::parse_from_rfc3339("2023-03-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(Interval::Days.num_rotations(then, t0)?, 1);
        counter.increment_then(then, 3)?;
        assert_eq!(counter.data.buckets[2], 3u64);

        // Out of bounds
        // t0 + 1d12h
        let then = DateTime::parse_from_rfc3339("2023-03-11T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(Interval::Days.num_rotations(then, t0)?, -1);
        assert!(counter.increment_then(then, 1).is_err());

        // t0 - 1y
        let then = DateTime::parse_from_rfc3339("2022-03-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(Interval::Days.num_rotations(then, t0)?, 365);
        assert!(counter.increment_then(then, 1).is_ok());
        // buckets.len() grows up to a maximum of bucket_count.
        assert_eq!(counter.data.buckets.len(), 3);

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
        counter.increment(1).ok();

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
        let date = Utc::now() + Duration::minutes(1);
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

    #[test]
    fn test_advance_minutes_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:30Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-11T00:01:20Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Minutes)
                .unwrap()
                .data
                .buckets[1],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_hours_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-11T01:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Hours)
                .unwrap()
                .data
                .buckets[1],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_days_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-12T01:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter.intervals.get(&Interval::Days).unwrap().data.buckets[1],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_weeks_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-06-19T01:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Weeks)
                .unwrap()
                .data
                .buckets[1],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_months_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2022-08-02T01:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets[1],
            1
        );
        Ok(())
    }

    #[test]
    fn test_advance_years_relative() -> Result<()> {
        let d1 = DateTime::parse_from_rfc3339("2022-06-10T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let d2 = DateTime::parse_from_rfc3339("2024-06-02T01:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut counter: MultiIntervalCounter = Default::default();
        counter
            .intervals
            .iter_mut()
            .for_each(|(_, c)| c.data.starting_instant = d1);
        counter.increment(1)?;
        counter.maybe_advance(d2)?;

        assert_eq!(
            counter
                .intervals
                .get(&Interval::Years)
                .unwrap()
                .data
                .buckets[1],
            1
        );
        Ok(())
    }
}

#[cfg(test)]
mod event_store_tests {
    use chrono::{Datelike, Duration};

    use crate::NimbusTargetingHelper;

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

        let mut store = EventStore::from(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        let tmp_dir = tempfile::tempdir()?;
        let db = Database::new(&tmp_dir)?;

        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(2)))?;
        store.persist_data(&db)?;

        // Rebuild the EventStore from persisted data in order to test persistence
        let store = EventStore::try_from(&db)?;
        dbg!("From persisted data: {:?}", &store);

        assert_eq!(
            store
                .events
                .get("event-1")
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
                .get("event-1")
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
                .get("event-1")
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
                .get("event-1")
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
                .get("event-1")
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
                .get("event-1")
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
                .get("event-2")
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
                .get("event-2")
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
    fn record_event_should_create_events_where_applicable() -> Result<()> {
        let mut store = EventStore::new();
        store.record_event(1, "test", None)?;

        assert_eq!(
            store
                .events
                .get("test")
                .unwrap()
                .intervals
                .get(&Interval::Minutes)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            store
                .events
                .get("test")
                .unwrap()
                .intervals
                .get(&Interval::Hours)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            store
                .events
                .get("test")
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
                .get("test")
                .unwrap()
                .intervals
                .get(&Interval::Weeks)
                .unwrap()
                .data
                .buckets[0],
            1
        );
        assert_eq!(
            store
                .events
                .get("test")
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
                .get("test")
                .unwrap()
                .intervals
                .get(&Interval::Years)
                .unwrap()
                .data
                .buckets[0],
            1
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

        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(2)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query("event-1", Interval::Days, 7, 0, EventQueryType::Sum)?,
            3.0
        );
        assert_eq!(
            store.query("event-1", Interval::Days, 0, 0, EventQueryType::Sum)?,
            0.0
        );
        assert_eq!(
            store.query("event-1", Interval::Days, 7, 7, EventQueryType::Sum)?,
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

        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(2)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1",
                Interval::Days,
                7,
                0,
                EventQueryType::CountNonZero
            )?,
            2.0
        );
        assert_eq!(
            store.query(
                "event-1",
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

        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(2)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1",
                Interval::Days,
                7,
                0,
                EventQueryType::AveragePerInterval
            )?,
            0.42857142857142855
        );
        assert_eq!(
            store.query(
                "event-1",
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

        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(2)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;
        store.record_event(1, "event-1", Some(Utc::now() + Duration::days(3)))?;

        assert_eq!(
            store.query(
                "event-1",
                Interval::Days,
                7,
                0,
                EventQueryType::AveragePerNonZeroInterval
            )?,
            1.5
        );
        assert_eq!(
            store.query(
                "event-1",
                Interval::Days,
                7,
                2,
                EventQueryType::AveragePerNonZeroInterval
            )?,
            0.0
        );

        Ok(())
    }

    #[test]
    fn query_last_seen_should_function() -> Result<()> {
        let counter1 = MultiIntervalCounter::new(vec![
            SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
        ]);

        let mut store = EventStore::from(vec![("event-1".to_string(), counter1)]);
        store.record_past_event(1, "event-1", None, Duration::days(10))?;
        store.record_past_event(1, "event-1", None, Duration::days(20))?;

        assert_eq!(
            store.query(
                "event-1",
                Interval::Days,
                usize::MAX,
                0,
                EventQueryType::LastSeen
            )?,
            10.0
        );
        assert_eq!(
            store.query("event-1", Interval::Days, 365, 21, EventQueryType::LastSeen)?,
            f64::MAX
        );

        Ok(())
    }

    #[test]
    fn record_past_event_should_reflect_in_queries() -> Result<()> {
        let mut store: EventStore = Default::default();
        let event_id = "app_launch";
        let delta = Duration::days(1);
        let now = Utc::now();
        let then = now - delta;
        store.record_past_event(5, event_id, None, delta)?;

        // eventSum, for all intervals
        assert_eq!(
            0.0,
            store.query(event_id, Interval::Minutes, 60, 0, EventQueryType::Sum)?
        );
        assert_eq!(
            0.0,
            store.query(event_id, Interval::Hours, 24, 0, EventQueryType::Sum)?
        );
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::Sum)?
        );
        if same_year(now, then) {
            assert_eq!(
                5.0,
                store.query(
                    event_id,
                    Interval::Weeks,
                    1,
                    usize::from(!same_week(now, then)), // 0 if the same week, 1 if not
                    EventQueryType::Sum
                )?
            );
            assert_eq!(
                5.0,
                store.query(
                    event_id,
                    Interval::Months,
                    1,
                    usize::from(!same_month(now, then)), // 0 if the same month, 1 if not
                    EventQueryType::Sum
                )?
            );
            assert_eq!(
                5.0,
                store.query(event_id, Interval::Years, 1, 0, EventQueryType::Sum)?
            );
        }

        // If we want to be absolute timings, use the interval of
        // with greater than the granularity that the event was recorded.
        // You need to give up on some precision to avoid the dance of
        // week days and months.
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Weeks, 2, 0, EventQueryType::Sum)?
        );
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Months, 2, 0, EventQueryType::Sum)?
        );
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Years, 2, 0, EventQueryType::Sum)?
        );

        // If we want to be precise and relative, use the interval of
        // with less than or equal to the granularity that the event was recorded.
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Days, 1, 1, EventQueryType::Sum)?
        );
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Hours, 1, 24, EventQueryType::Sum)?
        );

        // Mark out when the event was not, in order to prove we're recording the
        // event in the correct time.
        assert_eq!(
            5.0,
            store.query(event_id, Interval::Days, 1, 1, EventQueryType::Sum)?
        );

        assert_eq!(
            f64::MAX,
            store.query(event_id, Interval::Hours, 24, 0, EventQueryType::LastSeen)?
        );
        assert_eq!(
            1.0,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::LastSeen)?
        );

        if same_year(now, then) {
            // We disable this test for the first few days of January
            // because the same_week fn uses the day_of_year.
            // We use day_of_year because we treat weeks as relative to the
            // first day of the year, which could be any day.
            assert_eq!(
                if same_week(now, then) {
                    // Same week.
                    0.0
                } else {
                    // Last week.
                    1.0
                },
                store.query(event_id, Interval::Weeks, 52, 0, EventQueryType::LastSeen)?
            );
            assert_eq!(
                if same_month(now, then) {
                    // Same month
                    0.0
                } else {
                    // Last month
                    1.0
                },
                store.query(event_id, Interval::Months, 12, 0, EventQueryType::LastSeen)?
            );
            assert_eq!(
                0.0,
                store.query(event_id, Interval::Years, 12, 0, EventQueryType::LastSeen)?
            );
        }
        let mut store: EventStore = Default::default();
        let delta = Duration::weeks(1);
        let now = Utc::now();
        let then = now - delta;
        store.record_past_event(5, event_id, None, delta)?;
        assert_eq!(
            f64::MAX,
            store.query(event_id, Interval::Hours, 24, 0, EventQueryType::LastSeen)?
        );
        assert_eq!(
            7.0,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::LastSeen)?
        );
        assert_eq!(
            1.0,
            store.query(event_id, Interval::Weeks, 52, 0, EventQueryType::LastSeen)?
        );
        assert_eq!(
            (now.year() - then.year()) as f64,
            store.query(event_id, Interval::Years, 12, 0, EventQueryType::LastSeen)?
        );

        Ok(())
    }

    fn same_week(now: DateTime<Utc>, then: DateTime<Utc>) -> bool {
        let now_week_num = now.ordinal0() / 7;
        let then_week_num = then.ordinal0() / 7;

        now_week_num == then_week_num
    }

    fn same_month(now: DateTime<Utc>, then: DateTime<Utc>) -> bool {
        let now_num = now.ordinal0() / 28;
        let then_num = then.ordinal0() / 28;

        now_num == then_num
    }

    fn same_year(now: DateTime<Utc>, then: DateTime<Utc>) -> bool {
        now.year() == then.year()
    }

    #[test]
    fn advancing_datum_should_reflect_in_queries() -> Result<()> {
        let mut store: EventStore = Default::default();
        let event_id = "app_launch";
        let num_days = 10;
        let delta = Duration::days(num_days);

        // Record now
        store.record_event(2, event_id, None)?;
        // Advance 10 days.
        store.advance_datum(delta);
        // Test that the last time we saw an event was 10 days ago.
        assert_eq!(
            num_days as f64,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::LastSeen)?
        );

        // Record an event 2 days in the past.
        let num_days = 2;
        store.record_past_event(1, event_id, None, Duration::days(num_days))?;
        // Test that the last time we saw an event was 10 days ago.
        assert_eq!(
            num_days as f64,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::LastSeen)?,
        );

        assert_eq!(
            3f64,
            store.query(event_id, Interval::Days, 56, 0, EventQueryType::Sum)?
        );

        Ok(())
    }

    #[test]
    fn test_days_weeks() -> Result<()> {
        let one_day = Duration::days(1);
        let mut now = Utc::now();

        for _ in 1..10 {
            let then = now - one_day;
            println!(
                "today is {}, yesterday is {}, same week? {}",
                now.weekday(),
                then.weekday(),
                same_week(now, then)
            );
            now += one_day;
        }

        Ok(())
    }

    #[test]
    fn test_integration_smoke_test() -> Result<()> {
        let mut store = EventStore::default();
        let event_id = "dummy_event";

        store.record_event(1, event_id, None)?;

        let th = NimbusTargetingHelper::from(store);

        assert!(
            th.eval_jexl(format!("'{event_id}'|eventSum('Minutes') >= 0"))
                .is_err()
        );
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Minutes', 1) == 1"))?);
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Minutes', 1, 1) == 0"))?);

        // This is one minute bucket 24h ago. We error out at zero.
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Minutes', 1, 24 * 60) == 0"))?);
        // This is the last 24 hours of one minute buckets. This is the same as the first 60.
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Minutes', 24 * 60) == 1"))?);

        assert!(
            th.eval_jexl(format!("'{event_id}'|eventSum('Years') >= 0"))
                .is_err()
        );
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Years', 1) == 1"))?);
        assert!(th.eval_jexl(format!("'{event_id}'|eventSum('Years', 1, 1) == 0"))?);

        assert!(
            th.eval_jexl(format!("'{event_id}'|eventCountNonZero('Minutes') >= 0"))
                .is_err()
        );
        assert!(th.eval_jexl(format!("'{event_id}'|eventCountNonZero('Minutes', 1) == 1"))?);
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventCountNonZero('Minutes', 1, 1) == 0"
        ))?);

        assert!(
            th.eval_jexl(format!(
                "'{event_id}'|eventAveragePerInterval('Minutes') >= 0"
            ))
            .is_err()
        );
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventAveragePerInterval('Minutes', 1) == 1"
        ))?);
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventAveragePerInterval('Minutes', 1, 1) == 0"
        ))?);

        assert!(
            th.eval_jexl(format!(
                "'{event_id}'|eventAveragePerNonZeroInterval('Minutes') >= 0"
            ))
            .is_err()
        );
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventAveragePerNonZeroInterval('Minutes', 1) >= 0"
        ))?);
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventAveragePerNonZeroInterval('Minutes', 1, 1) >= 0"
        ))?);

        // When was this event last seen? It was seen zero minutes ago.
        assert!(th.eval_jexl(format!("'{event_id}'|eventLastSeen('Minutes') == 0"))?);
        // Before this last minute, when was this event last seen? It was at least 60 minutes ago, if ever
        assert!(th.eval_jexl(format!("'{event_id}'|eventLastSeen('Minutes', 1) > 60"))?);
        // LastSeen doesn't support a fourth argument.
        assert!(
            th.eval_jexl(format!("'{event_id}'|eventLastSeen('Minutes', 1, 1) >= 0"))
                .is_err()
        );

        // Q: Before 24 hours ago, when did we last see this event?
        // A: it was greater than 24h, but likely never.
        assert!(th.eval_jexl(format!(
            "'{event_id}'|eventLastSeen('Minutes', 24 * 60) > 24 * 60"
        ))?);

        Ok(())
    }
}

#[cfg(test)]
mod event_query_type_tests {
    use super::*;
    use crate::stateful::behavior::EventQueryType;

    #[test]
    fn test_extract_query() -> Result<()> {
        assert!(EventQueryType::validate_query(
            "'event'|eventSum('Years', 28, 0)"
        )?);
        assert!(EventQueryType::validate_query(
            "'event'|eventCountNonZero('Months', 28, 0)"
        )?);
        assert!(EventQueryType::validate_query(
            "'event'|eventAverage('Weeks', 28, 0)"
        )?);
        assert!(EventQueryType::validate_query(
            "'event'|eventAveragePerNonZeroInterval('Days', 28, 0)"
        )?);
        assert!(EventQueryType::validate_query(
            "'event'|eventLastSeen('Hours', 10)"
        )?);
        assert!(EventQueryType::validate_query(
            "'event'|eventSum('Minutes', 86400, 0)"
        )?);
        assert!(!EventQueryType::validate_query("yolo")?);
        Ok(())
    }
}
