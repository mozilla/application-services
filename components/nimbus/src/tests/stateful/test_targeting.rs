// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{Map, json};

use crate::stateful::behavior::EventStore;
use crate::stateful::targeting::{execute_event_queries, validate_event_queries};
use crate::tests::helpers::TestRecordedContext;
use crate::{AppContext, NimbusTargetingHelper, Result, TargetingAttributes};

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
    let helper = create_helper(json!({}));

    let result = helper.eval_jexl_debug("invalid {{").unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(!parsed["error"].as_str().unwrap().is_empty());
}

#[test]
fn test_eval_jexl_debug_json_structure() {
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
fn test_eval_jexl_debug_returns_json() -> Result<()> {
    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct EvalResult {
        success: bool,
        result: serde_json::Value,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct EvalError {
        success: bool,
        error: String,
    }

    let helper = create_helper(json!({"locale": "en-US"}));

    assert_eq!(
        serde_json::from_str::<EvalResult>(&helper.eval_jexl_debug("locale == 'en-US'")?)?,
        EvalResult {
            success: true,
            result: json!(true),
        }
    );

    assert_eq!(
        serde_json::from_str::<EvalResult>(&helper.eval_jexl_debug("locale")?)?,
        EvalResult {
            success: true,
            result: json!("en-US")
        },
    );

    assert_eq!(
        serde_json::from_str::<EvalError>(&helper.eval_jexl_debug("bogus")?)?,
        EvalError {
            success: false,
            error: "EvaluationError: Identifier 'bogus' is undefined".into(),
        }
    );

    Ok(())
}

#[test]
fn test_eval_jexl_debug_with_version_compare() {
    let app_ctx = AppContext {
        app_version: Some("115.0".to_string()),
        ..Default::default()
    };

    let targeting_attributes: TargetingAttributes = app_ctx.into();
    let helper = create_helper(serde_json::to_value(&targeting_attributes).unwrap());

    let result = helper
        .eval_jexl_debug("app_version|versionCompare('114.0') > 0")
        .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["result"], true);
}
