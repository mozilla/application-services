// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Map, json};

use crate::stateful::behavior::EventStore;
use crate::stateful::targeting::{execute_event_queries, validate_event_queries};
use crate::tests::helpers::TestRecordedContext;
use crate::{NimbusTargetingHelper, Result, TargetingAttributes};

#[test]
fn test_recorded_context_cannot_shadow_enrollment_state() -> Result<()> {
    let mut attributes = TargetingAttributes::default();
    attributes.active_experiments.insert("real-exp".to_string());
    attributes
        .enrollments_map
        .insert("real-exp".to_string(), "control".to_string());

    // A context that echoes stale enrollment state back, alongside an ordinary key.
    attributes.set_recorded_context(
        json!({
            "active_experiments": ["stale-exp"],
            "enrollments": ["stale-exp"],
            "enrollments_map": { "stale-exp": "treatment" },
            "is_already_enrolled": true,
            "other": "kept",
        })
        .as_object()
        .expect("not an object")
        .clone(),
    );

    // Dropped when stored, so this does not depend on which field serializes last.
    let stored = attributes.recorded_context.as_ref().expect("no context");
    for key in [
        "active_experiments",
        "enrollments",
        "enrollments_map",
        "is_already_enrolled",
    ] {
        assert!(!stored.contains_key(key), "{key} was kept");
    }

    let targeting = serde_json::to_value(&attributes)?;
    assert_eq!(targeting["active_experiments"], json!(["real-exp"]));
    assert_eq!(targeting["enrollments"], json!([]));
    assert_eq!(
        targeting["enrollments_map"],
        json!({ "real-exp": "control" })
    );
    assert_eq!(targeting["is_already_enrolled"], json!(false));
    assert_eq!(targeting["other"], json!("kept"));

    Ok(())
}

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
    execute_event_queries(&*recorded_context, &targeting_helper)?;

    assert_eq!(
        recorded_context.get_event_query_values()["TEST_QUERY_SUCCESS"],
        1.0
    );
    assert!(
        !recorded_context
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
    let result = validate_event_queries(recorded_context);
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

    let result = helper.eval_jexl_debug("locale == 'en-US'").unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["result"], true);
}

#[test]
fn test_eval_jexl_debug_error() {
    use serde_json::json;

    let helper = create_helper(json!({}));

    let result = helper.eval_jexl_debug("invalid {{").unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(!parsed["error"].as_str().unwrap().is_empty());
}

#[test]
fn test_eval_jexl_debug_json_structure() {
    use serde_json::json;

    let helper = create_helper(json!({"test": true}));

    let result = helper.eval_jexl_debug("test").unwrap();

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

    let result = helper.eval_jexl_debug("locale").unwrap();

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
        .eval_jexl_debug("app_version|versionCompare('114.0') > 0")
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["result"], true);
}
