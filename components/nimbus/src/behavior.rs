#![allow(dead_code)]

use crate::error::{BehaviorError, NimbusError, Result};
use chrono::{DateTime, Timelike, Utc};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub enum Interval {
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

pub enum IntervalName {
    Example1,
    Example2,
}

impl IntervalName {
    fn value(&self) -> String {
        match *self {
            IntervalName::Example1 => "example1".to_string(),
            IntervalName::Example2 => "example2".to_string(),
        }
    }
}

impl PartialEq for IntervalName {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}
impl Eq for IntervalName {}

impl Hash for IntervalName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value().as_bytes().hash(state);
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
    fn new(bucket_count: usize, interval: Interval) -> Self {
        Self {
            bucket_count,
            interval,
        }
    }
}

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
            buckets: VecDeque::new(),
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

    fn increment(&mut self) -> Result<()> {
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

    fn rotate(&mut self, num_rotations: u64) -> Result<()> {
        for _ in 1..=num_rotations {
            self.buckets.push_front(0)
        }
        if self.buckets.len() > self.bucket_count {
            self.buckets.drain(self.bucket_count..);
        }
        Ok(())
    }
}

fn date_diff(a: DateTime<Utc>, b: DateTime<Utc>) -> u32 {
    (a.date() - b.date()).num_days().try_into().unwrap()
}

struct SingleIntervalCounter {
    data: IntervalData,
    config: IntervalConfig,
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

    pub fn increment(&mut self) -> Result<()> {
        self.data.increment()
    }

    pub fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        let then = self.data.starting_instant;
        let rotations: u32 = match self.config.interval {
            Interval::Minutes => now.minute() - then.minute(),
            Interval::Hours => now.hour() - then.hour(),
            Interval::Days => date_diff(now, then),
            Interval::Weeks => date_diff(now, then) / 7,
            Interval::Months => date_diff(now, then) / 28,
            Interval::Years => date_diff(now, then) / 365,
        };
        if rotations > 0 {
            return self.data.rotate(rotations.into());
        }
        Ok(())
    }
}

struct MultiIntervalCounter {
    intervals: HashMap<IntervalName, SingleIntervalCounter>,
}

impl MultiIntervalCounter {
    pub fn new(intervals: Vec<(IntervalName, SingleIntervalCounter)>) -> Self {
        Self {
            intervals: HashMap::from_iter(intervals.into_iter()),
        }
    }

    pub fn from(intervals: HashMap<IntervalName, SingleIntervalCounter>) -> Self {
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

#[cfg(test)]
mod date_diff_tests {
    use super::*;

    #[test]
    fn diffs_dates_ignoring_time() -> Result<()> {
        let date1 = DateTime::parse_from_rfc3339("2022-10-26T08:00:00Z").unwrap();
        let date2 = DateTime::parse_from_rfc3339("2022-10-27T01:00:00Z").unwrap();
        assert!(matches!(
            date_diff(date2.with_timezone(&Utc), date1.with_timezone(&Utc)),
            1
        ));
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
            (
                IntervalName::Example1,
                SingleIntervalCounter::new(IntervalConfig::new(7, Interval::Days)),
            ),
            (
                IntervalName::Example2,
                SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
            ),
        ]);
        counter.increment().ok();

        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example1)
                .unwrap()
                .data
                .buckets[0],
            1
        ));
        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example2)
                .unwrap()
                .data
                .buckets[0],
            1
        ));
        Ok(())
    }

    #[test]
    fn test_advance_do_not_advance() -> Result<()> {
        let mut counter = MultiIntervalCounter::new(vec![
            (
                IntervalName::Example1,
                SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            ),
            (
                IntervalName::Example2,
                SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
            ),
        ]);
        let date = Utc::now();
        counter.maybe_advance(date).ok();

        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example1)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        ));
        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example2)
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
            (
                IntervalName::Example1,
                SingleIntervalCounter::new(IntervalConfig::new(12, Interval::Months)),
            ),
            (
                IntervalName::Example2,
                SingleIntervalCounter::new(IntervalConfig::new(28, Interval::Days)),
            ),
        ]);
        let date = Utc::now() + Duration::days(1);
        counter.maybe_advance(date).ok();

        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example1)
                .unwrap()
                .data
                .buckets
                .len(),
            1
        ));
        assert!(matches!(
            counter
                .intervals
                .get(&IntervalName::Example2)
                .unwrap()
                .data
                .buckets
                .len(),
            2
        ));
        Ok(())
    }
}
