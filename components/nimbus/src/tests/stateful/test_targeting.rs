// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    stateful::{behavior::EventStore, targeting::RecordedContext},
    tests::helpers::TestRecordedContext,
    NimbusTargetingHelper, Result,
};
use serde_json::Map;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[test]
fn test_recorded_context_execute_queries() -> Result<()> {
    let mut event_store = EventStore::new();
    event_store.record_event(1, "event", None)?;
    let event_store = Arc::new(Mutex::new(event_store));
    let targeting_helper = NimbusTargetingHelper::new(Map::new(), event_store, None);

    let map = HashMap::from_iter(vec![
        (
            "TEST_QUERY_SUCCESS".to_string(),
            "'event'|eventSum('Days', 1, 0)".into(),
        ),
        (
            "TEST_QUERY_FAIL_NOT_VALID_QUERY".to_string(),
            "'event'|eventYolo('Days', 1, 0)".into(),
        ),
    ]);

    let recorded_context = TestRecordedContext::new();
    recorded_context.set_event_queries(map.clone());
    let recorded_context: Box<dyn RecordedContext> = Box::new(recorded_context);

    recorded_context.execute_queries(&targeting_helper)?;

    // SAFETY: The cast to TestRecordedContext is safe because the Rust instance is
    // guaranteed to be a TestRecordedContext instance. TestRecordedContext is the only
    // Rust-implemented version of RecordedContext, and, like this method,  is only
    // used in tests.
    let test_recorded_context = unsafe {
        std::mem::transmute::<&&dyn RecordedContext, &&TestRecordedContext>(&&*recorded_context)
    };
    assert_eq!(
        test_recorded_context.get_event_query_values()["TEST_QUERY_SUCCESS"],
        1.0
    );
    assert!(!test_recorded_context
        .get_event_query_values()
        .contains_key("TEST_QUERY_FAIL_NOT_VALID_QUERY"));

    Ok(())
}

#[test]
fn test_recorded_context_validate_queries() -> Result<()> {
    let map = HashMap::from_iter(vec![
        (
            "TEST_QUERY_SUCCESS".to_string(),
            "'event'|eventSum('Days', 1, 0)".into(),
        ),
        (
            "TEST_QUERY_FAIL_NOT_VALID_QUERY".to_string(),
            "'event'|eventYolo('Days', 1, 0)".into(),
        ),
    ]);

    let recorded_context = TestRecordedContext::new();
    recorded_context.set_event_queries(map.clone());
    let recorded_context: Box<dyn RecordedContext> = Box::new(recorded_context);

    let result = recorded_context.validate_queries();
    assert!(result.is_err_and(|e| {
        assert_eq!(e.to_string(), "Behavior error: EventQueryParseError: \"'event'|eventYolo('Days', 1, 0)\" is not a valid EventQuery".to_string());
        true
    }));

    Ok(())
}
