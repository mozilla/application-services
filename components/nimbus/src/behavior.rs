#![allow(dead_code)]

use crate::error::{BehaviorError, NimbusError, Result};
use chrono::{DateTime, Timelike, Utc};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub enum Interval {
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl PartialEq for Interval {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}
impl Eq for Interval {}

impl Hash for Interval {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_string().as_bytes().hash(state);
    }
}

#[derive(Clone)]
struct IntervalConfig {
    bucket_count: usize,
    interval: Interval,
}

impl Default for IntervalConfig {
    fn default() -> Self {
        Self::new(7, Interval::Days)
    }
}

impl IntervalConfig {
    pub fn new(bucket_count: usize, interval: Interval) -> Self {
        Self {
            bucket_count,
            interval,
        }
    }
}

#[derive(Clone)]
pub struct IntervalData {
    buckets: VecDeque<u64>,
    bucket_count: usize,
    starting_instant: DateTime<Utc>,
}

impl Default for IntervalData {
    fn default() -> Self {
        Self::new(1)
    }
}

impl IntervalData {
    fn new(bucket_count: usize) -> Self {
        let mut data = Self {
            buckets: VecDeque::with_capacity(bucket_count),
            bucket_count,
            starting_instant: Utc::now(),
        };
        data.buckets.push_front(0);
        data
    }

    pub fn from(
        buckets: VecDeque<u64>,
        bucket_count: usize,
        starting_instant: DateTime<Utc>,
    ) -> Self {
        Self {
            buckets,
            bucket_count,
            starting_instant,
        }
    }

    pub fn increment(&mut self) -> Result<()> {
        match self.buckets.front_mut() {
            Some(x) => *x += 1,
            None => {
                return Err(NimbusError::BehaviorError(BehaviorError::InvalidState(
                    "Interval buckets cannot be empty".to_string(),
                )))
            }
        };
        Ok(())
    }

    pub fn rotate(&mut self, num_rotations: i32) -> Result<()> {
        let num_rotations = usize::min(self.bucket_count, num_rotations as usize);
        if num_rotations as usize + self.buckets.len() > self.bucket_count {
            self.buckets.drain((self.bucket_count - num_rotations)..);
        }
        for _ in 1..=num_rotations {
            self.buckets.push_front(0);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct SingleIntervalCounter {
    pub data: IntervalData,
    pub config: IntervalConfig,
}

impl SingleIntervalCounter {
    pub fn new(config: IntervalConfig) -> Self {
        Self {
            data: IntervalData::new(config.bucket_count),
            config,
        }
    }

    pub fn from(data: IntervalData, config: IntervalConfig) -> Self {
        Self { data, config }
    }

    pub fn from_config(bucket_count: usize, interval: Interval) -> Self {
        let config = IntervalConfig {
            bucket_count,
            interval,
        };
        Self::new(config)
    }

    pub fn increment(&mut self) -> Result<()> {
        self.data.increment()
    }

    fn num_rotations(&self, now: DateTime<Utc>) -> Result<i32> {
        let hour_diff = i32::try_from(self.data.starting_instant.hour() - now.hour())?;
        let date_diff = i32::try_from((now.date() - self.data.starting_instant.date()).num_days())?;
        Ok(match self.config.interval {
            Interval::Hours => (date_diff * 24) + hour_diff,
            Interval::Days => date_diff,
            Interval::Weeks => date_diff / 7,
            Interval::Months => date_diff / 28,
            Interval::Years => date_diff / 365,
        })
    }

    pub fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        let rotations = self.num_rotations(now)?;
        self.data.starting_instant = now;
        if rotations > 0 {
            return self.data.rotate(rotations);
        }
        Ok(())
    }
}

struct MultiIntervalCounter {
    intervals: HashMap<Interval, SingleIntervalCounter>,
}

impl MultiIntervalCounter {
    pub fn new(intervals: Vec<SingleIntervalCounter>) -> Self {
        Self {
            intervals: intervals
                .into_iter()
                .map(|v| (v.config.interval.clone(), v))
                .collect::<HashMap<Interval, SingleIntervalCounter>>(),
        }
    }

    pub fn from(intervals: HashMap<Interval, SingleIntervalCounter>) -> Self {
        Self { intervals }
    }

    pub fn increment(&mut self) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.increment())
    }

    pub fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.maybe_advance(now))
    }
}

type EventId = String;

struct EventStore {
    events: HashMap<EventId, MultiIntervalCounter>,
}

impl EventStore {
    pub fn new(events: Vec<(EventId, MultiIntervalCounter)>) -> Self {
        Self {
            events: HashMap::from_iter(events.into_iter()),
        }
    }

    pub fn from(events: HashMap<EventId, MultiIntervalCounter>) -> Self {
        Self { events }
    }

    pub fn record_event(&mut self, event_id: EventId, now: Option<DateTime<Utc>>) -> Result<()> {
        let now = now.unwrap_or_else(Utc::now);
        let counter = self.events.get_mut(&event_id).unwrap();
        counter.maybe_advance(now)?;
        counter.increment()
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

        assert!(matches!(result.is_err(), true));
        Ok(())
    }

    #[test]
    fn increment_increments_front_bucket_if_it_exists() -> Result<()> {
        let mut interval = IntervalData::new(7);
        interval.increment().ok();

        assert!(matches!(interval.buckets[0], 1));
        Ok(())
    }

    #[test]
    fn rotate_adds_buckets_for_each_rotation() -> Result<()> {
        let mut interval = IntervalData::new(7);
        interval.rotate(3).ok();

        assert!(matches!(interval.buckets.len(), 4));
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

        assert!(matches!(interval.buckets.len(), 3));
        assert!(matches!(interval.buckets[0], 2));
        assert!(matches!(interval.buckets[1], 1));
        assert!(matches!(interval.buckets[2], 0));
        Ok(())
    }

    #[test]
    fn rotate_handles_large_rotation() -> Result<()> {
        let mut interval = IntervalData::new(3);
        interval.rotate(10).ok();

        assert!(matches!(interval.buckets.len(), 3));
        assert!(matches!(interval.buckets[0], 0));
        assert!(matches!(interval.buckets[1], 0));
        assert!(matches!(interval.buckets[2], 0));
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

        assert!(matches!(counter.data.buckets[0], 1));
        Ok(())
    }

    #[test]
    fn test_advance_do_not_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days));
        let date = Utc::now();
        counter.maybe_advance(date).ok();

        assert!(matches!(counter.data.buckets.len(), 1));
        Ok(())
    }

    #[test]
    fn test_advance_do_advance() -> Result<()> {
        let mut counter = SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days));
        let date = Utc::now() + Duration::days(1);
        counter.maybe_advance(date).ok();

        assert!(matches!(counter.data.buckets.len(), 2));
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

        assert!(matches!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets[0],
            1
        ));
        assert!(matches!(
            counter.intervals.get(&Interval::Days).unwrap().data.buckets[0],
            1
        ));
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

        assert!(matches!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        ));
        assert!(matches!(
            counter
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        ));
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

        assert!(matches!(
            counter
                .intervals
                .get(&Interval::Months)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        ));
        assert!(matches!(
            counter
                .intervals
                .get(&Interval::Days)
                .unwrap()
                .data
                .buckets
                .len(),
            2
        ));
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

        let mut store = EventStore::new(vec![
            ("event-1".to_string(), counter1),
            ("event-2".to_string(), counter2),
        ]);

        store.record_event("event-1".to_string(), Some(Utc::now() + Duration::days(2)))?;

        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));

        assert!(matches!(
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
        ));
        assert!(matches!(
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
        ));

        Ok(())
    }
}
