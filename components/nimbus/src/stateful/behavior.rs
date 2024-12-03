/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    error::{BehaviorError, NimbusError, Result},
    stateful::persistence::{Database, StoreId},
};
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::vec_deque::Iter;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interval {
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

impl Interval {
    pub fn num_rotations(&self, then: DateTime<Utc>, now: DateTime<Utc>) -> Result<i32> {
        let date_diff = now - then;
        Ok(i32::try_from(match self {
            Interval::Minutes => date_diff.num_minutes(),
            Interval::Hours => date_diff.num_hours(),
            Interval::Days => date_diff.num_days(),
            Interval::Weeks => date_diff.num_weeks(),
            Interval::Months => date_diff.num_days() / 28,
            Interval::Years => date_diff.num_days() / 365,
        })?)
    }

    pub fn to_duration(&self, count: i64) -> Duration {
        match self {
            Interval::Minutes => Duration::minutes(count),
            Interval::Hours => Duration::hours(count),
            Interval::Days => Duration::days(count),
            Interval::Weeks => Duration::weeks(count),
            Interval::Months => Duration::days(28 * count),
            Interval::Years => Duration::days(365 * count),
        }
    }
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

impl FromStr for Interval {
    type Err = NimbusError;

    fn from_str(input: &str) -> Result<Self> {
        Ok(match input {
            "Minutes" => Self::Minutes,
            "Hours" => Self::Hours,
            "Days" => Self::Days,
            "Weeks" => Self::Weeks,
            "Months" => Self::Months,
            "Years" => Self::Years,
            _ => {
                return Err(NimbusError::BehaviorError(
                    BehaviorError::IntervalParseError(input.to_string()),
                ));
            }
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IntervalConfig {
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

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct IntervalData {
    pub(crate) buckets: VecDeque<u64>,
    pub(crate) bucket_count: usize,
    pub(crate) starting_instant: DateTime<Utc>,
}

impl Default for IntervalData {
    fn default() -> Self {
        Self::new(1)
    }
}

impl IntervalData {
    pub fn new(bucket_count: usize) -> Self {
        let mut buckets = VecDeque::with_capacity(bucket_count);
        buckets.push_front(0);
        // Set the starting instant to Jan 1 00:00:00 in order to sync rotations
        let starting_instant = Utc.from_utc_datetime(
            &Utc::now()
                .with_month(1)
                .unwrap()
                .with_day(1)
                .unwrap()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        );
        Self {
            buckets,
            bucket_count,
            starting_instant,
        }
    }

    pub fn increment(&mut self, count: u64) -> Result<()> {
        self.increment_at(0, count)
    }

    pub fn increment_at(&mut self, index: usize, count: u64) -> Result<()> {
        if index < self.bucket_count {
            let buckets = &mut self.buckets;
            match buckets.get_mut(index) {
                Some(x) => *x += count,
                None => {
                    for _ in buckets.len()..index {
                        buckets.push_back(0);
                    }
                    self.buckets.insert(index, count)
                }
            };
        }
        Ok(())
    }

    pub fn rotate(&mut self, num_rotations: i32) -> Result<()> {
        let num_rotations = usize::min(self.bucket_count, num_rotations as usize);
        if num_rotations + self.buckets.len() > self.bucket_count {
            self.buckets.drain((self.bucket_count - num_rotations)..);
        }
        for _ in 1..=num_rotations {
            self.buckets.push_front(0);
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SingleIntervalCounter {
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

    pub fn from_config(bucket_count: usize, interval: Interval) -> Self {
        let config = IntervalConfig {
            bucket_count,
            interval,
        };
        Self::new(config)
    }

    pub fn increment_then(&mut self, then: DateTime<Utc>, count: u64) -> Result<()> {
        use std::cmp::Ordering;
        let now = self.data.starting_instant;
        let rotations = self.config.interval.num_rotations(then, now)?;
        match rotations.cmp(&0) {
            Ordering::Less => {
                /* We can't increment in the future */
                return Err(NimbusError::BehaviorError(BehaviorError::InvalidState(
                    "Cannot increment events far into the future".to_string(),
                )));
            }
            Ordering::Equal => {
                if now < then {
                    self.data.increment_at(0, count)?;
                } else {
                    self.data.increment_at(1, count)?;
                }
            }
            Ordering::Greater => self.data.increment_at(1 + rotations as usize, count)?,
        }
        Ok(())
    }

    pub fn increment(&mut self, count: u64) -> Result<()> {
        self.data.increment(count)
    }

    pub fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        let rotations = self
            .config
            .interval
            .num_rotations(self.data.starting_instant, now)?;
        if rotations > 0 {
            self.data.starting_instant += self.config.interval.to_duration(rotations.into());
            return self.data.rotate(rotations);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MultiIntervalCounter {
    pub intervals: HashMap<Interval, SingleIntervalCounter>,
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

    pub fn increment_then(&mut self, then: DateTime<Utc>, count: u64) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.increment_then(then, count))
    }

    pub fn increment(&mut self, count: u64) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.increment(count))
    }

    pub fn maybe_advance(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.intervals
            .iter_mut()
            .try_for_each(|(_, v)| v.maybe_advance(now))
    }
}

impl Default for MultiIntervalCounter {
    fn default() -> Self {
        Self::new(vec![
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 60,
                interval: Interval::Minutes,
            }),
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 72,
                interval: Interval::Hours,
            }),
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 56,
                interval: Interval::Days,
            }),
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 52,
                interval: Interval::Weeks,
            }),
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 12,
                interval: Interval::Months,
            }),
            SingleIntervalCounter::new(IntervalConfig {
                bucket_count: 4,
                interval: Interval::Years,
            }),
        ])
    }
}

#[derive(Debug)]
pub enum EventQueryType {
    Sum,
    CountNonZero,
    AveragePerInterval,
    AveragePerNonZeroInterval,
    LastSeen,
}

impl fmt::Display for EventQueryType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl EventQueryType {
    pub fn perform_query(&self, buckets: Iter<u64>, num_buckets: usize) -> Result<f64> {
        Ok(match self {
            Self::Sum => buckets.sum::<u64>() as f64,
            Self::CountNonZero => buckets.filter(|v| v > &&0u64).count() as f64,
            Self::AveragePerInterval => buckets.sum::<u64>() as f64 / num_buckets as f64,
            Self::AveragePerNonZeroInterval => {
                let values = buckets.fold((0, 0), |accum, item| {
                    (
                        accum.0 + item,
                        if item > &0 { accum.1 + 1 } else { accum.1 },
                    )
                });
                if values.1 == 0 {
                    0.0
                } else {
                    values.0 as f64 / values.1 as f64
                }
            }
            Self::LastSeen => match buckets.into_iter().position(|v| v > &0) {
                Some(v) => v as f64,
                None => f64::MAX,
            },
        })
    }

    fn validate_counting_arguments(
        &self,
        args: &[Value],
    ) -> Result<(String, Interval, usize, usize)> {
        if args.len() < 3 || args.len() > 4 {
            return Err(NimbusError::TransformParameterError(format!(
                "event transform {} requires 2-3 parameters",
                self
            )));
        }
        let event = match serde_json::from_value::<String>(args.first().unwrap().clone()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("event = nimbus::stateful::behavior::EventQueryType::validate_counting_arguments::serde_json::from_value".into(), e.to_string()))
        };
        let interval = match serde_json::from_value::<String>(args.get(1).unwrap().clone()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("interval = nimbus::stateful::behavior::EventQueryType::validate_counting_arguments::serde_json::from_value".into(), e.to_string()))
        };
        let interval = Interval::from_str(&interval)?;
        let num_buckets = match args.get(2).unwrap().as_f64() {
            Some(v) => v,
            None => {
                return Err(NimbusError::TransformParameterError(format!(
                    "event transform {} requires a positive number as the second parameter",
                    self
                )));
            }
        } as usize;
        let zero = &Value::from(0);
        let starting_bucket = match args.get(3).unwrap_or(zero).as_f64() {
            Some(v) => v,
            None => {
                return Err(NimbusError::TransformParameterError(format!(
                    "event transform {} requires a positive number as the third parameter",
                    self
                )));
            }
        } as usize;

        Ok((event, interval, num_buckets, starting_bucket))
    }

    fn validate_last_seen_arguments(
        &self,
        args: &[Value],
    ) -> Result<(String, Interval, usize, usize)> {
        if args.len() < 2 || args.len() > 3 {
            return Err(NimbusError::TransformParameterError(format!(
                "event transform {} requires 1-2 parameters",
                self
            )));
        }
        let event = match serde_json::from_value::<String>(args.first().unwrap().clone()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("event = nimbus::stateful::behavior::EventQueryType::validate_last_seen_arguments::serde_json::from_value".into(), e.to_string()))
        };
        let interval = match serde_json::from_value::<String>(args.get(1).unwrap().clone()) {
            Ok(v) => v,
            Err(e) => return Err(NimbusError::JSONError("interval = nimbus::stateful::behavior::EventQueryType::validate_last_seen_arguments::serde_json::from_value".into(), e.to_string()))
        };
        let interval = Interval::from_str(&interval)?;
        let zero = &Value::from(0);
        let starting_bucket = match args.get(2).unwrap_or(zero).as_f64() {
            Some(v) => v,
            None => {
                return Err(NimbusError::TransformParameterError(format!(
                    "event transform {} requires a positive number as the second parameter",
                    self
                )));
            }
        } as usize;

        Ok((
            event,
            interval,
            usize::MAX - starting_bucket,
            starting_bucket,
        ))
    }

    pub fn validate_arguments(&self, args: &[Value]) -> Result<(String, Interval, usize, usize)> {
        // `args` is an array of values sent by the evaluator for a JEXL transform.
        // The first parameter will always be the event_id, and subsequent parameters are up to the developer's discretion.
        // All parameters should be validated, and a `TransformParameterError` should be sent when there is an error.
        Ok(match self {
            Self::Sum
            | Self::CountNonZero
            | Self::AveragePerInterval
            | Self::AveragePerNonZeroInterval => self.validate_counting_arguments(args)?,
            Self::LastSeen => self.validate_last_seen_arguments(args)?,
        })
    }

    pub fn validate_query(maybe_query: &str) -> Result<bool> {
        let regex = regex::Regex::new(
            r#"^(?:"[^"']+"|'[^"']+')\|event(?:Sum|LastSeen|CountNonZero|Average|AveragePerNonZeroInterval)\(["'](?:Years|Months|Weeks|Days|Hours|Minutes)["'],\s*\d+\s*(?:,\s*\d+\s*)?\)$"#,
        )?;
        Ok(regex.is_match(maybe_query))
    }

    fn error_value(&self) -> f64 {
        match self {
            Self::LastSeen => f64::MAX,
            _ => 0.0,
        }
    }
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct EventStore {
    pub(crate) events: HashMap<String, MultiIntervalCounter>,
    datum: Option<DateTime<Utc>>,
}

impl From<Vec<(String, MultiIntervalCounter)>> for EventStore {
    fn from(event_store: Vec<(String, MultiIntervalCounter)>) -> Self {
        Self {
            events: HashMap::from_iter(event_store),
            datum: None,
        }
    }
}

impl From<HashMap<String, MultiIntervalCounter>> for EventStore {
    fn from(event_store: HashMap<String, MultiIntervalCounter>) -> Self {
        Self {
            events: event_store,
            datum: None,
        }
    }
}

impl TryFrom<&Database> for EventStore {
    type Error = NimbusError;

    fn try_from(db: &Database) -> Result<Self, NimbusError> {
        let reader = db.read()?;
        let events = db
            .get_store(StoreId::EventCounts)
            .collect_all::<(String, MultiIntervalCounter), _>(&reader)?;
        Ok(EventStore::from(events))
    }
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            events: HashMap::<String, MultiIntervalCounter>::new(),
            datum: None,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.datum.unwrap_or_else(Utc::now)
    }

    pub fn advance_datum(&mut self, duration: Duration) {
        self.datum = Some(self.now() + duration);
    }

    pub fn read_from_db(&mut self, db: &Database) -> Result<()> {
        let reader = db.read()?;

        self.events =
            HashMap::from_iter(
                db.get_store(StoreId::EventCounts)
                    .collect_all::<(String, MultiIntervalCounter), _>(&reader)?,
            );

        Ok(())
    }

    pub fn record_event(
        &mut self,
        count: u64,
        event_id: &str,
        now: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let now = now.unwrap_or_else(|| self.now());
        let counter = self.get_or_create_counter(event_id);
        counter.maybe_advance(now)?;
        counter.increment(count)
    }

    pub fn record_past_event(
        &mut self,
        count: u64,
        event_id: &str,
        now: Option<DateTime<Utc>>,
        duration: Duration,
    ) -> Result<()> {
        let now = now.unwrap_or_else(|| self.now());
        let then = now - duration;
        let counter = self.get_or_create_counter(event_id);
        counter.maybe_advance(now)?;
        counter.increment_then(then, count)
    }

    fn get_or_create_counter(&mut self, event_id: &str) -> &mut MultiIntervalCounter {
        if !self.events.contains_key(event_id) {
            let new_counter = Default::default();
            self.events.insert(event_id.to_string(), new_counter);
        }
        self.events.get_mut(event_id).unwrap()
    }

    pub fn persist_data(&self, db: &Database) -> Result<()> {
        let mut writer = db.write()?;
        self.events.iter().try_for_each(|(key, value)| {
            db.get_store(StoreId::EventCounts)
                .put(&mut writer, key, &(key.clone(), value.clone()))
        })?;
        writer.commit()?;
        Ok(())
    }

    pub fn clear(&mut self, db: &Database) -> Result<()> {
        self.events = HashMap::<String, MultiIntervalCounter>::new();
        self.datum = None;
        self.persist_data(db)?;
        Ok(())
    }

    pub fn query(
        &mut self,
        event_id: &str,
        interval: Interval,
        num_buckets: usize,
        starting_bucket: usize,
        query_type: EventQueryType,
    ) -> Result<f64> {
        let now = self.now();
        if let Some(counter) = self.events.get_mut(event_id) {
            counter.maybe_advance(now)?;
            if let Some(single_counter) = counter.intervals.get(&interval) {
                let safe_range = 0..single_counter.data.buckets.len();
                if !safe_range.contains(&starting_bucket) {
                    return Ok(query_type.error_value());
                }
                let max = usize::min(
                    num_buckets + starting_bucket,
                    single_counter.data.buckets.len(),
                );
                let buckets = single_counter.data.buckets.range(starting_bucket..max);
                return query_type.perform_query(buckets, num_buckets);
            }
        }
        Ok(query_type.error_value())
    }
}

pub fn query_event_store(
    event_store: Arc<Mutex<EventStore>>,
    query_type: EventQueryType,
    args: &[Value],
) -> Result<Value> {
    let (event, interval, num_buckets, starting_bucket) = query_type.validate_arguments(args)?;

    Ok(json!(event_store.lock().unwrap().query(
        &event,
        interval,
        num_buckets,
        starting_bucket,
        query_type,
    )?))
}
