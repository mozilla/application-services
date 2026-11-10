// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde_json::{Map, Value};

use std::collections::HashMap;

#[cfg(feature = "stateful")]
use crate::{NimbusError, Result};

#[cfg(feature = "stateful")]
pub type JsonObject = Map<String, Value>;

#[cfg(feature = "stateful")]
pub type PrefValue = Value;

/// Replace any instance of [from] with [to] in any string within the [serde_json::Value].
///
/// This recursively descends into the object, looking at string values and keys.
#[allow(dead_code)]
pub(crate) fn replace_str(value: &mut Value, from: &str, to: &str) {
    let replacer = create_str_replacer(from, to);
    replace_str_with(value, &replacer);
}

/// Replace any instance of [from] with [to] in any string within the [serde_json::Value::Map].
///
/// This recursively descends into the object, looking at string values and keys.
pub(crate) fn replace_str_in_map(map: &mut Map<String, Value>, from: &str, to: &str) {
    let replacer = create_str_replacer(from, to);
    replace_str_in_map_with(map, &replacer);
}

fn replace_str_with<F>(value: &mut Value, replacer: &F)
where
    F: Fn(&str) -> Option<String> + ?Sized,
{
    match value {
        Value::String(s) => {
            if let Some(r) = replacer(s) {
                *s = r;
            }
        }

        Value::Array(list) => {
            for item in list.iter_mut() {
                replace_str_with(item, replacer);
            }
        }

        Value::Object(map) => {
            replace_str_in_map_with(map, replacer);
        }

        _ => (),
    };
}

pub(crate) fn replace_str_in_map_with<F>(map: &mut Map<String, Value>, replacer: &F)
where
    F: Fn(&str) -> Option<String> + ?Sized,
{
    // Replace values in place.
    for v in map.values_mut() {
        replace_str_with(v, replacer);
    }

    // Replacing keys in place is a little trickier.
    let mut changes = HashMap::new();
    for k in map.keys() {
        if let Some(new) = replacer(k) {
            changes.insert(k.to_owned(), new);
        }
    }

    for (k, new) in changes {
        let v = map.remove(&k).unwrap();
        _ = map.insert(new, v);
    }
}

fn create_str_replacer<'a>(from: &'a str, to: &'a str) -> impl Fn(&str) -> Option<String> + 'a {
    move |s: &str| -> Option<String> {
        if s.contains(from) {
            Some(s.replace(from, to))
        } else {
            None
        }
    }
}

#[cfg(feature = "stateful")]
pub(crate) fn replace_l10n_in_map(
    map: &mut Map<String, Value>,
    translations: &HashMap<String, String>,
) -> Result<()> {
    for value in map.values_mut() {
        replace_l10n(value, translations)?;
    }
    Ok(())
}

#[cfg(feature = "stateful")]
fn replace_l10n(value: &mut Value, translations: &HashMap<String, String>) -> Result<()> {
    match value {
        Value::Array(list) => {
            for item in list.iter_mut() {
                replace_l10n(item, translations)?;
            }
        }

        Value::Object(map) => {
            if let Some(l10n_value) = map.get("$l10n") {
                let Value::Object(l10n) = l10n_value else {
                    return Err(NimbusError::LocalizationError(
                        "Invalid $l10n object".into(),
                    ));
                };
                let Some(Value::String(id)) = l10n.get("id") else {
                    return Err(NimbusError::LocalizationError(
                        "Missing or invalid $l10n id".into(),
                    ));
                };
                let Some(text) = translations.get(id) else {
                    return Err(NimbusError::LocalizationError(format!(
                        "Missing translation for {id}"
                    )));
                };
                *value = Value::String(text.clone());
            } else {
                replace_l10n_in_map(map, translations)?;
            }
        }

        _ => (),
    };
    Ok(())
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

    #[cfg(feature = "stateful")]
    #[test]
    fn test_replace_l10n() {
        let mut value = json!({
            "text": {
                "$l10n": {
                    "id": "welcome",
                    "text": "Welcome!",
                    "comment": "test comment"
                }
            }
        });
        let mut translations = HashMap::new();
        translations.insert(String::from("welcome"), String::from("Bienvenue!"));

        replace_l10n_in_map(value.as_object_mut().unwrap(), &translations).unwrap();

        assert_eq!(value, json!({"text": "Bienvenue!"}));
    }

    #[cfg(feature = "stateful")]
    #[test]
    fn test_replace_l10n_malformed() {
        let mut translations = HashMap::new();
        translations.insert(String::from("welcome"), String::from("Bienvenue!"));

        let mut value = json!({"text": {"$l10n": "welcome"}});
        let result = replace_l10n_in_map(value.as_object_mut().unwrap(), &translations);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Localization error: Invalid $l10n object"
        );

        let mut value = json!({"text": {"$l10n": {"text": "Welcome!"}}});
        let result = replace_l10n_in_map(value.as_object_mut().unwrap(), &translations);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Localization error: Missing or invalid $l10n id"
        );

        let mut value = json!({"text": {"$l10n": {"id": "test-missing", "text": "Welcome!"}}});
        let result = replace_l10n_in_map(value.as_object_mut().unwrap(), &translations);
        assert_eq!(
            result.unwrap_err().to_string(),
            "Localization error: Missing translation for test-missing"
        );
    }
}
