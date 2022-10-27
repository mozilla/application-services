#![allow(dead_code)]

use crate::error::{BehaviorError, NimbusError, Result};
use chrono::{DateTime, Timelike, Utc};
use std::collections::{HashMap, VecDeque};

#[derive(Clone)]
enum Interval {
    MINUTES,
    HOURS,
    DAYS,
    WEEKS,
    MONTHS,
    YEARS,
}

#[derive(Clone)]
struct IntervalConfig {
    bucket_count: usize,
    interval: Interval,
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
    pub fn new(bucket_count: usize) -> IntervalData {
        let mut data = IntervalData {
            buckets: VecDeque::new(),
            bucket_count: bucket_count,
            starting_instant: Utc::now(),
        };
        data.buckets.push_front(0);
        data
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
            self.buckets.drain((self.bucket_count - 1)..);
        }
        Ok(())
    }
}

struct SingleIntervalCounter {
    data: IntervalData,
    config: IntervalConfig,
}

fn date_diff(a: DateTime<Utc>, b: DateTime<Utc>) -> u32 {
    (a.date() - b.date()).num_days().try_into().unwrap()
}

impl SingleIntervalCounter {
    fn new(interval_config: IntervalConfig) -> SingleIntervalCounter {
        SingleIntervalCounter {
            data: IntervalData::new(interval_config.bucket_count.clone()),
            config: interval_config.clone(),
        }
    }

    fn increment(&mut self) -> Result<()> {
        self.data.increment()
    }

    fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        let then = self.data.starting_instant;
        let rotations: u32 = match self.config.interval {
            Interval::MINUTES => now.minute() - then.minute(),
            Interval::HOURS => now.hour() - then.hour(),
            Interval::DAYS => date_diff(now, then),
            Interval::WEEKS => date_diff(now, then) / 7,
            Interval::MONTHS => date_diff(now, then) / 28,
            Interval::YEARS => date_diff(now, then) / 365,
        };
        if rotations > 0 {
            return self.data.rotate(rotations.into());
        }
        Ok(())
    }
}

struct MultiIntervalCounter {
    intervals: HashMap<i64, SingleIntervalCounter>,
}

impl MultiIntervalCounter {
    fn increment(&mut self) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.increment())
    }

    fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
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
        let result = interval.increment();

        assert!(matches!(result.is_err(), false));
        assert!(matches!(interval.buckets[0], 1));
        Ok(())
    }

    #[test]
    fn rotate_adds_buckets_for_each_rotation() -> Result<()> {
        let mut interval = IntervalData::new(7);
        let result = interval.rotate(3);

        assert!(matches!(result.is_err(), false));
        assert!(matches!(interval.buckets.len(), 4));
        Ok(())
    }
}
