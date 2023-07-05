// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde_json::{Map, Value};
use std::collections::HashMap;

pub(crate) fn replace_str(value: &mut Value, pattern: &str, slug: &str) {
    match value {
        Value::String(s) => {
            if s.contains(pattern) {
                *s = s.replace(pattern, slug);
            }
        }

        Value::Array(list) => {
            for item in list.iter_mut() {
                replace_str(item, pattern, slug)
            }
        }

        Value::Object(map) => {
            replace_str_in_map(map, pattern, slug);
        }

        _ => (),
    };
}

pub(crate) fn replace_str_in_map(map: &mut Map<String, Value>, pattern: &str, slug: &str) {
    // Replace values in place.
    for v in map.values_mut() {
        replace_str(v, pattern, slug);
    }

    // Replacing keys in place is a little trickier.
    let mut changes = HashMap::new();
    for k in map.keys() {
        if k.contains(pattern) {
            let new = k.replace(pattern, slug);
            changes.insert(k.to_owned(), new);
        }
    }

    for (k, new) in changes {
        let v = map.remove(&k).unwrap();
        _ = map.insert(new, v);
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_replace_str() {
        let mut value = json!("{test}");
        replace_str(&mut value, "{test}", "success");
        assert_eq!(value, json!("success"));

        let mut value = json!("{test}-postfix");
        replace_str(&mut value, "{test}", "success");
        assert_eq!(value, json!("success-postfix"));

        let mut value = json!("prefix-{test}");
        replace_str(&mut value, "{test}", "success");
        assert_eq!(value, json!("prefix-success"));

        let mut value = json!("prefix-{test}-postfix");
        replace_str(&mut value, "{test}", "success");
        assert_eq!(value, json!("prefix-success-postfix"));

        let mut value = json!("prefix-{test}-multi-{test}-postfix");
        replace_str(&mut value, "{test}", "success");
        assert_eq!(value, json!("prefix-success-multi-success-postfix"));
    }

    #[test]
    fn test_replace_str_in_array() {
        let mut value = json!(["alice", "bob", "{placeholder}", "daphne"]);
        replace_str(&mut value, "{placeholder}", "charlie");
        assert_eq!(value, json!(["alice", "bob", "charlie", "daphne"]));
    }

    #[test]
    fn test_replace_str_in_map() {
        let mut value = json!({
            "key": "{test}",
            "not": true,
            "or": 2,
        });
        replace_str(&mut value, "{test}", "success");
        assert_eq!(
            value,
            json!({
                "key": "success",
                "not": true,
                "or": 2,
            })
        );
    }

    #[test]
    fn test_replace_str_in_map_keys() {
        let mut value = json!({
            "{test}-en-US": "{test}",
            "not": true,
            "or": 2,
        });
        replace_str(&mut value, "{test}", "success");
        assert_eq!(
            value,
            json!({
                "success-en-US": "success",
                "not": true,
                "or": 2,
            })
        );
    }

    #[test]
    fn test_replace_str_mixed() {
        let mut value = json!({
            "messages": {
                "{test}-en-US": {
                    "test": "{test}"
                },
                "{test}{test}": {
                    "test": "{test}{test}"
                }
            }
        });
        replace_str(&mut value, "{test}", "success");
        assert_eq!(
            value,
            json!({
                "messages": {
                    "success-en-US": {
                        "test": "success"
                    },
                    "successsuccess": {
                        "test": "successsuccess"
                    }
                }
            })
        );
    }
}
