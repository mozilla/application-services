/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{NimbusError, Result};
use serde_json::{value::Value, Map};

#[allow(dead_code)]
pub fn fmt<T: serde::Serialize>(template: &str, context: &T) -> Result<String> {
    let obj: Value = match serde_json::to_value(context) {
        Ok(v) => v,
        Err(e) => {
            return Err(NimbusError::JSONError(
                "obj = nimbus::strings::fmt::serde_json::to_value".into(),
                e.to_string(),
            ))
        }
    };

    fmt_with_value(template, &obj)
}

#[allow(dead_code)]
pub fn fmt_with_value(template: &str, value: &Value) -> Result<String> {
    if let Value::Object(map) = value {
        Ok(fmt_with_map(template, map))
    } else {
        Err(NimbusError::EvaluationError(
            "Can only format json objects".to_string(),
        ))
    }
}

pub fn fmt_with_map(input: &str, context: &Map<String, Value>) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut output = String::with_capacity(input.len());

    let mut iter = input.grapheme_indices(true);
    let mut last_index = 0;

    // This is exceedingly simple; never refer to this as a parser.
    while let Some((index, c)) = iter.next() {
        if c == "{" {
            let open_index = index;
            for (index, c) in iter.by_ref() {
                if c == "}" {
                    let close_index = index;
                    let field_name = &input[open_index + 1..close_index];

                    // If we decided to embed JEXL into this templating language,
                    // this would be the place to put it.
                    // However, we'd likely want to make this be able to detect balanced braces,
                    // which this does not.
                    let replace_string = match context.get(field_name) {
                        Some(Value::Bool(v)) => v.to_string(),
                        Some(Value::String(v)) => v.to_string(),
                        Some(Value::Number(v)) => v.to_string(),
                        _ => format!("{{{v}}}", v = field_name),
                    };

                    output.push_str(&input[last_index..open_index]);
                    output.push_str(&replace_string);

                    // +1 skips the closing }
                    last_index = close_index + 1;
                    break;
                }
            }
        }
    }

    output.push_str(&input[last_index..input.len()]);

    output
}

#[cfg(test)]
mod unit_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn smoke_tests() {
        let c = json!({
            "string": "STRING".to_string(),
            "number": 42,
            "boolean": true,
        });
        let c = c.as_object().unwrap();

        assert_eq!(
            fmt_with_map("A {string}, a {number}, a {boolean}.", c),
            "A STRING, a 42, a true.".to_string()
        );
    }

    #[test]
    fn test_unicode_boundaries() {
        let c = json!({
            "empty": "".to_string(),
            "unicode": "a̐éö̲".to_string(),
            "a̐éö̲": "unicode".to_string(),
        });
        let c = c.as_object().unwrap();

        assert_eq!(fmt_with_map("fîré{empty}ƒøüX", c), "fîréƒøüX".to_string());
        assert_eq!(fmt_with_map("a̐éö̲{unicode}a̐éö̲", c), "a̐éö̲a̐éö̲a̐éö̲".to_string());
        assert_eq!(
            fmt_with_map("is this {a̐éö̲}?", c),
            "is this unicode?".to_string()
        );
    }

    #[test]
    fn test_pathological_cases() {
        let c = json!({
            "empty": "".to_string(),
        });
        let c = c.as_object().unwrap();

        assert_eq!(
            fmt_with_map("A {notthere}.", c),
            "A {notthere}.".to_string()
        );
        assert_eq!(
            fmt_with_map("aa { unclosed", c),
            "aa { unclosed".to_string()
        );
    }
}
