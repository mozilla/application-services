/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use crate::enrollment::NotEnrolledReason;
use crate::evaluator::targeting;
use crate::stateful::behavior::{
    EventStore, Interval, IntervalConfig, IntervalData, MultiIntervalCounter, SingleIntervalCounter,
};
use crate::stateful::targeting::RecordedContext;
use crate::tests::helpers::TestRecordedContext;
use crate::{AppContext, EnrollmentStatus, TargetingAttributes};

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
            reason: "EvaluationError: Custom error: JSON Error: event = nimbus::stateful::behavior::EventQueryType::validate_counting_arguments::serde_json::from_value — invalid type: floating point `1.0`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventSum(1, 3, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: interval = nimbus::stateful::behavior::EventQueryType::validate_counting_arguments::serde_json::from_value — invalid type: floating point `1.0`, expected a string"
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
            reason: "EvaluationError: Custom error: JSON Error: event = nimbus::stateful::behavior::EventQueryType::validate_last_seen_arguments::serde_json::from_value — invalid type: floating point `1.0`, expected a string"
                .to_string()
        })
    );
    assert_eq!(
        targeting(
            "'app.foregrounded'|eventLastSeen(1, 0) > 1",
            &th,
        ),
        Some(EnrollmentStatus::Error {
            reason: "EvaluationError: Custom error: JSON Error: interval = nimbus::stateful::behavior::EventQueryType::validate_last_seen_arguments::serde_json::from_value — invalid type: floating point `1.0`, expected a string"
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

    assert_eq!(
        targeting("'app_cycle.foreground1'|eventLastSeen('Days', 2) > 1", &th),
        None
    );
}

#[test]
fn test_targeting_active_experiments_equivalency() {
    // Here's our valid jexl statement
    let expression_statement = "'test' in active_experiments";
    // A matching context that includes the appropriate specific context
    let mut targeting_attributes: TargetingAttributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    let mut set = HashSet::<String>::new();
    set.insert("test".into());
    targeting_attributes.active_experiments = set;

    // The targeting should pass!
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.clone().into()),
        None
    );

    // We set active_experiment treatment to something not expected and try again
    let mut set = HashSet::<String>::new();
    set.insert("test1".into());
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.clone().into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );

    // We set active_experiments to None and try again
    let set = HashSet::<String>::new();
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_targeting_active_experiments_exists() {
    // Here's our valid jexl statement
    let expression_statement = "'test' in active_experiments";
    // A matching context that includes the appropriate specific context
    let mut targeting_attributes: TargetingAttributes = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        architecture: Some("x86_64".to_string()),
        device_manufacturer: Some("Samsung".to_string()),
        device_model: Some("Galaxy S10".to_string()),
        locale: Some("en-US".to_string()),
        os: Some("Android".to_string()),
        os_version: Some("10".to_string()),
        android_sdk_version: Some("29".to_string()),
        debug_tag: None,
        custom_targeting_attributes: None,
        ..Default::default()
    }
    .into();
    let mut set = HashSet::<String>::new();
    set.insert("test".into());
    targeting_attributes.active_experiments = set;

    // The targeting should pass!
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.clone().into()),
        None
    );

    // We set active_experiment treatment to something not expected and try again
    let set = HashSet::<String>::new();
    targeting_attributes.active_experiments = set;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_targeting_is_already_enrolled() {
    // Here's our valid jexl statement
    let expression_statement = "is_already_enrolled";
    // A matching context that includes the appropriate specific context
    let ac = AppContext {
        app_name: "nimbus_test".to_string(),
        app_id: "1010".to_string(),
        channel: "test".to_string(),
        app_version: Some("4.4".to_string()),
        app_build: Some("1234".to_string()),
        custom_targeting_attributes: None,
        ..Default::default()
    };
    let mut targeting_attributes = TargetingAttributes::from(ac);
    targeting_attributes.is_already_enrolled = true;

    // The targeting should pass!
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.clone().into(),),
        None
    );

    // We make the is_already_enrolled false and try again
    targeting_attributes.is_already_enrolled = false;
    assert_eq!(
        targeting(expression_statement, &targeting_attributes.into()),
        Some(EnrollmentStatus::NotEnrolled {
            reason: NotEnrolledReason::NotTargeted
        })
    );
}

#[test]
fn test_bucket_sample() {
    let cases = [
        ("1.1", "1000", "1000", None),
        (
            "0",
            "1.1",
            "1000",
            Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            }),
        ),
        (
            "0",
            "0",
            "1000.1",
            Some(EnrollmentStatus::NotEnrolled {
                reason: NotEnrolledReason::NotTargeted,
            }),
        ),
        (
            "4294967296",
            "1",
            "4294967297",
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: start is out of range".into(),
            }),
        ),
        (
            "0",
            "4294967296",
            "4294967296",
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: count is out of range".into(),
            }),
        ),
        (
            "0",
            "0",
            "4294967296",
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: total is out of range".into(),
            }),
        ),
        (
            r#""hello""#,
            "0",
            "1000",
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: start is not a number".into(),
            }),
        ),
        (
            "0",
            r#""hello""#,
            "1000",
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: count is not a number".into(),
            }),
        ),
        (
            "0",
            "1000",
            r#""hello""#,
            Some(EnrollmentStatus::Error {
                reason: "EvaluationError: Custom error: total is not a number".into(),
            }),
        ),
    ];

    for (start, count, total, expected) in cases {
        let expr = format!("0|bucketSample({start}, {count}, {total})");
        println!("{}", expr);
        let targeting_attributes: TargetingAttributes = AppContext {
            app_name: "nimbus_test".into(),
            app_id: "nimbus-test".into(),
            channel: "test".into(),
            ..Default::default()
        }
        .into();

        let result = targeting(&expr, &targeting_attributes.clone().into());

        assert_eq!(result, expected);
    }
}

#[test]
fn test_multiple_contexts_flatten() -> crate::Result<()> {
    let recorded_context = Arc::new(TestRecordedContext::new());
    recorded_context.set_context(json!({
        "locale": "de-CA",
        "language": "de",
    }));
    let mut targeting_attributes =
        crate::tests::test_evaluator::ta_with_locale("en-US".to_string());
    targeting_attributes.set_recorded_context(recorded_context.to_json());

    let value = serde_json::to_value(targeting_attributes).unwrap();

    assert_eq!(value.get("locale").unwrap(), "de-CA");
    assert_eq!(value.get("language").unwrap(), "de");

    Ok(())
}
