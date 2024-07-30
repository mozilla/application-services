// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    stateful::{behavior::EventStore, targeting::RecordedContext},
    tests::helpers::TestRecordedContext,
    NimbusTargetingHelper, Result,
};
use serde_json::{json, Map};
use std::sync::{Arc, Mutex};

#[test]
fn test_recorded_context_execute_queries() -> Result<()> {
    let mut event_store = EventStore::new();
    event_store.record_event(1, "event", None)?;
    let event_store = Arc::new(Mutex::new(event_store));
    let targeting_helper = NimbusTargetingHelper::new(Map::new(), event_store);

    let map = Map::from_iter(vec![
        (
            "TEST_QUERY_SUCCESS".to_string(),
            json!("'event'|eventSum('Days', 1, 0)"),
        ),
        (
            "TEST_QUERY_FAIL_NOT_VALID_QUERY".to_string(),
            json!("'event'|eventYolo('Days', 1, 0)"),
        ),
    ]);

    let recorded_context = TestRecordedContext::new();
    recorded_context.set_event_queries(map.clone());
    let recorded_context: Box<dyn RecordedContext> = Box::new(recorded_context);

    let result = recorded_context.execute_queries(&targeting_helper)?;
    assert_eq!(result["TEST_QUERY_SUCCESS"], json!(1.0));
    assert!(result
        .get("TEST_QUERY_FAIL_NOT_VALID_QUERY")
        .unwrap()
        .is_string());

    Ok(())
}
