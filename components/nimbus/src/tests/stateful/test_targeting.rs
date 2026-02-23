// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Map;

use crate::stateful::{behavior::EventStore, targeting::RecordedContext};
use crate::tests::helpers::TestRecordedContext;
use crate::{NimbusTargetingHelper, Result};

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
    assert!(
        !test_recorded_context
            .get_event_query_values()
            .contains_key("TEST_QUERY_FAIL_NOT_VALID_QUERY")
    );

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

fn create_helper(context: serde_json::Value) -> NimbusTargetingHelper {
    NimbusTargetingHelper::new(context, Arc::new(Mutex::new(EventStore::new())), None)
}

#[test]
fn test_eval_jexl_debug_success() {
    use serde_json::json;

    let helper = create_helper(json!({
        "locale": "en-US",
    }));

    let result = helper
        .eval_jexl_debug("locale == 'en-US'".to_string())
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["result"], true);
}

#[test]
fn test_eval_jexl_debug_error() {
    use serde_json::json;

    let helper = create_helper(json!({}));

    let result = helper.eval_jexl_debug("invalid {{".to_string()).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(!parsed["error"].as_str().unwrap().is_empty());
}

#[test]
fn test_eval_jexl_debug_json_structure() {
    use serde_json::json;

    let helper = create_helper(json!({"test": true}));

    let result = helper.eval_jexl_debug("test".to_string()).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    // Verify JSON structure for success case
    assert!(parsed.is_object());
    assert_eq!(parsed["success"], true);
    assert!(parsed.get("result").is_some());
    assert!(parsed.get("error").is_none());
}

#[test]
fn test_eval_jexl_debug_returns_pretty_json() {
    use serde_json::json;

    let helper = create_helper(json!({"locale": "en-US"}));

    let result = helper.eval_jexl_debug("locale".to_string()).unwrap();

    // Pretty JSON should have newlines
    assert!(result.contains('\n'));
    // Should be valid JSON
    assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
}

#[test]
fn test_eval_jexl_debug_with_version_compare() {
    use crate::AppContext;

    let app_ctx = AppContext {
        app_version: Some("115.0".to_string()),
        ..Default::default()
    };

    let targeting_attributes: crate::TargetingAttributes = app_ctx.into();
    let helper = create_helper(serde_json::to_value(&targeting_attributes).unwrap());

    let result = helper
        .eval_jexl_debug("app_version|versionCompare('114.0') > 0".to_string())
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["result"], true);
}
