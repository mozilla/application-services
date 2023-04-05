/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    behavior::{
        EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter,
        SingleIntervalCounter,
    },
    enrollment::NotEnrolledReason,
    evaluator::targeting,
    EnrollmentStatus,
};
use chrono::Utc;

#[test]
fn test_event_sum_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: vec![1, 1, 0].into(),
        },
        config: IntervalConfig::new(3, Interval::Days),
    }]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let th = event_store.into();
    assert_eq!(
        targeting("'app.foregrounded'|eventSum('Days', 3, 0) > 2", &th),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting("'app.foregrounded'|eventSum('Days', 3, 0) > 1", &th,),
        None
    );
}

#[test]
fn test_event_count_non_zero_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: vec![1, 2, 0].into(),
        },
        config: IntervalConfig::new(3, Interval::Days),
    }]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let th = event_store.into();
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventCountNonZero('Days', 3, 0) > 2",
            &th,
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventCountNonZero('Days', 3, 0) > 1",
            &th,
        ),
        None
    );
}

#[test]
fn test_event_average_per_interval_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: vec![1, 2, 0, 0, 0, 2, 3].into(),
        },
        config: IntervalConfig::new(3, Interval::Days),
    }]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let th = event_store.into();
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventAveragePerInterval('Days', 7, 0) > 2",
            &th,
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventAveragePerInterval('Days', 7, 0) > 1.14",
            &th,
        ),
        None
    );
}

#[test]
fn test_event_average_per_non_zero_interval_transform() {
    let counter = MultiIntervalCounter::new(vec![SingleIntervalCounter {
        data: IntervalData {
            bucket_count: 3,
            starting_instant: Utc::now(),
            buckets: vec![1, 2, 0, 0, 0, 2, 4].into(),
        },
        config: IntervalConfig::new(3, Interval::Days),
    }]);

    let event_store = EventStore::from(vec![("app.foregrounded".to_string(), counter)]);
    let th = event_store.into();
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventAveragePerNonZeroInterval('Days', 7, 0) == 1",
            &th,
        ),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventAveragePerNonZeroInterval('Days', 7, 0) == 2.25",
            &th,
        ),
        None
    );
}

#[test]
fn test_event_transform_sum_cnz_avg_avgnz_parameters() {
    let th = Default::default();

    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum('Days') > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform Sum requires 2-3 parameters"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "1|eventSum('Days', 3, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum(1, 3, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum('Day', 3, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Behavior error: IntervalParseError: Day is not a valid Interval"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum('Days', 'test', 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform Sum requires a positive number as the second parameter"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum('Days', 3, 'test') > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform Sum requires a positive number as the third parameter"
                .to_string()
        })
    );
}

#[test]
fn test_event_transform_last_seen_parameters() {
    let th = Default::default();

    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen() > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform LastSeen requires 1-2 parameters"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen('Days', 0, 10) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform LastSeen requires 1-2 parameters"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "1|eventLastSeen('Days', 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen(1, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: invalid type: floating point `1`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen('Day', 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Behavior error: IntervalParseError: Day is not a valid Interval"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen('Days', 'test') > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: Transform parameter error: event transform LastSeen requires a positive number as the second parameter"
                .to_string()
        })
    );
}
