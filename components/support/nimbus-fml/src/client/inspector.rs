/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    error::{ClientError, FMLError, Result},
    intermediate_representation::FeatureManifest,
    FmlClient, JsonObject,
};
use serde_json::Value;
use std::sync::Arc;

impl FmlClient {
    pub fn get_feature_inspector(&self, id: String) -> Option<Arc<FmlFeatureInspector>> {
        _ = self.manifest.find_feature(&id)?;
        Some(Arc::new(FmlFeatureInspector::new(
            self.manifest.clone(),
            id,
        )))
    }
}

pub struct FmlFeatureInspector {
    manifest: Arc<FeatureManifest>,
    feature_id: String,
}

impl FmlFeatureInspector {
    pub(crate) fn new(manifest: Arc<FeatureManifest>, feature_id: String) -> Self {
        Self {
            manifest,
            feature_id,
        }
    }

    pub fn get_default_json(&self) -> Result<JsonObject> {
        match self
            .manifest
            .find_feature(&self.feature_id)
            .map(|(_, f)| f.default_json())
            // We know it's safe to unwrap here, because find_feature returns something, because we constructed
            // a inspector with the feature id.
            .unwrap()
        {
            Value::Object(map) => Ok(map),
            _ => Err(FMLError::ClientError(ClientError::InvalidFeatureValue(
                "A non-JSON object is returned as default. This is likely a Nimbus FML bug."
                    .to_string(),
            ))),
        }
    }

    pub fn is_feature_valid(&self, value: JsonObject) -> Result<bool> {
        self.manifest
            .validate_feature_config(&self.feature_id, serde_json::Value::Object(value))
            .map(|_| true)
    }

    pub fn get_first_error(&self, string: String) -> Option<FmlEditorError> {
        match self.get_syntax_error(&string) {
            Ok(json) => self.get_semantic_error(&string, json).err(),
            Err(err) => Some(err),
        }
    }

    pub fn get_errors(&self, string: String) -> Option<Vec<FmlEditorError>> {
        self.get_first_error(string).map(|e| vec![e])
    }
}

impl FmlFeatureInspector {
    fn get_syntax_error(&self, string: &str) -> Result<Value, FmlEditorError> {
        let json = serde_json::from_str::<Value>(string);
        if let Err(e) = json {
            let col = e.column();
            return Err(FmlEditorError {
                message: "Need valid JSON object".to_string(),
                // serde_json errors are 1 indexed.
                line: e.line() as u32 - 1,
                col: if col == 0 { 0 } else { col - 1 } as u32,
                highlight: None,
            });
        }
        let json = json.ok().unwrap();
        if json.is_object() {
            Ok(json)
        } else {
            Err(FmlEditorError {
                message: "Need valid JSON object".to_string(),
                line: 0,
                col: 0,
                highlight: Some(string.to_string()),
            })
        }
    }

    fn get_semantic_error(&self, src: &str, value: Value) -> Result<(), FmlEditorError> {
        self.manifest
            .validate_feature_config(&self.feature_id, value)
            .map_err(|e| match e {
                FMLError::FeatureValidationError {
                    literals, message, ..
                } => {
                    let highlight = literals.last().cloned();
                    let (line, col) = find_err(src, literals.into_iter());
                    FmlEditorError {
                        message,
                        line: line as u32,
                        col: col as u32,
                        highlight,
                    }
                }
                _ => {
                    unreachable!("Error {e:?} should be caught as FeatureValidationError");
                }
            })
            .map(|_| ())
    }
}

fn find_err(src: &str, path: impl Iterator<Item = String>) -> (usize, usize) {
    let mut lines = src.lines();

    let mut line_no = 0;
    let mut col_no = 0;

    let mut first_match = false;
    let mut cur = lines.next().unwrap_or_default();

    for p in path {
        loop {
            // If we haven't had our first match of the line, then start there at the beginning.
            // Otherwise, start one char on from where we were last time.
            let start = if !first_match { 0 } else { col_no + 1 };

            // if let Some(i) = cur[start..].find(&p).map(|i| i + start) {
            if let Some(i) = find_index(cur, &p, start) {
                col_no = i;
                first_match = true;
                break;
            } else if let Some(next) = lines.next() {
                // we try the next line!
                cur = next;
                line_no += 1;
                first_match = false;
                col_no = 0;
            } else {
                // we've run out of lines, so we should return
                return (0, 0);
            }
        }
    }

    (line_no, col_no)
}

fn find_index(cur: &str, pattern: &str, start: usize) -> Option<usize> {
    cur.match_indices(pattern)
        .find(|(i, _)| i >= &start)
        .map(|(i, _)| i)
}

#[derive(Debug, PartialEq)]
pub struct FmlEditorError {
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub highlight: Option<String>,
}

#[cfg(test)]
mod unit_tests {
    use crate::client::test_helper::client;

    use super::*;

    #[test]
    fn test_construction() -> Result<()> {
        let client = client("./nimbus_features.yaml", "release")?;
        assert_eq!(
            client.get_feature_ids(),
            vec!["dialog-appearance".to_string()]
        );
        let f = client.get_feature_inspector("dialog-appearance".to_string());
        assert!(f.is_some());

        let f = client.get_feature_inspector("not-there".to_string());
        assert!(f.is_none());

        Ok(())
    }

    fn error(message: &str, line: u32, col: u32, token: Option<&str>) -> FmlEditorError {
        FmlEditorError {
            message: message.to_string(),
            line,
            col,
            highlight: token.map(str::to_string),
        }
    }

    #[test]
    fn test_get_first_error_invalid_json() -> Result<()> {
        let client = client("./nimbus_features.yaml", "release")?;
        let f = client
            .get_feature_inspector("dialog-appearance".to_string())
            .unwrap();

        fn test_syntax_error(f: &FmlFeatureInspector, input: &str, col: u32, highlight: bool) {
            if let Some(e) = f.get_first_error(input.to_string()) {
                let highlight = if highlight { Some(input) } else { None };
                assert_eq!(e, error("Need valid JSON object", 0, col, highlight))
            } else {
                unreachable!("No error for \"{input}\"");
            }
        }

        test_syntax_error(&f, "", 0, false);
        test_syntax_error(&f, "x", 0, false);
        test_syntax_error(&f, "{ \"\" }, ", 5, false);
        test_syntax_error(&f, "{ \"foo\":", 7, false);

        test_syntax_error(&f, "[]", 0, true);
        test_syntax_error(&f, "1", 0, true);
        test_syntax_error(&f, "true", 0, true);
        test_syntax_error(&f, "\"string\"", 0, true);

        assert!(f.get_first_error("{}".to_string()).is_none());
        Ok(())
    }

    #[test]
    fn test_get_first_error_type_invalid() -> Result<()> {
        let client = client("./nimbus_features.yaml", "release")?;
        let f = client
            .get_feature_inspector("dialog-appearance".to_string())
            .unwrap();

        let s = r#"{}"#;
        assert!(f.get_first_error(s.to_string()).is_none());
        let s = r#"{
            "positive": {}
        }"#;
        assert!(f.get_first_error(s.to_string()).is_none());

        let s = r#"{
            "positive": 1
        }"#;
        if let Some(_err) = f.get_first_error(s.to_string()) {
        } else {
            unreachable!("No error for \"{s}\"");
        }

        let s = r#"{
            "positive1": {}
        }"#;
        if let Some(_err) = f.get_first_error(s.to_string()) {
        } else {
            unreachable!("No error for \"{s}\"");
        }

        Ok(())
    }

    #[test]
    fn test_find_err() -> Result<()> {
        fn do_test(s: &str, path: &[&str], expected: (usize, usize)) {
            let p = path.last().unwrap();
            let path = path.iter().map(|p| p.to_string());
            assert_eq!(
                find_err(s, path),
                expected,
                "Can't find \"{p}\" at {expected:?} in {s}"
            );
        }

        fn do_multi(s: &[&str], path: &[&str], expected: (usize, usize)) {
            let s = s.join("\n");
            do_test(&s, path, expected);
        }

        do_test("ab cd", &["ab", "cd"], (0, 3));

        do_test("ab ab", &["ab"], (0, 0));
        do_test("ab ab", &["ab", "ab"], (0, 3));

        do_multi(
            &["ab xx cd", "xx ef xx gh", "ij xx"],
            &["ab", "cd", "gh", "xx"],
            (2, 3),
        );

        do_multi(
            &[
                "{",                       // 0
                "  boolean: true,",        // 1
                "  object: {",             // 2
                "    integer: \"string\"", // 3
                "  }",                     // 4
                "}",                       // 5
            ],
            &["object", "integer", "\"string\""],
            (3, 13),
        );

        // pathological case
        do_multi(
            &[
                "{",                       // 0
                "  boolean: true,",        // 1
                "  object: {",             // 2
                "    integer: 1,",         // 3
                "    astring: \"string\"", // 4
                "  },",                    // 5
                "  integer: \"string\"",   // 6
                "}",                       // 7
            ],
            &["integer", "\"string\""],
            (4, 13),
        );

        Ok(())
    }

    #[test]
    fn test_find_index_from() -> Result<()> {
        assert_eq!(find_index("012345601", "01", 0), Some(0));
        assert_eq!(find_index("012345601", "01", 1), Some(7));
        assert_eq!(find_index("012345602", "01", 1), None);

        // TODO unicode indexing does not work.
        // assert_eq!(find_index("åéîø token", "token", 0), Some(5));
        Ok(())
    }

    #[test]
    fn test_deterministic_errors() -> Result<()> {
        let client = client("./nimbus_features.yaml", "release")?;
        let inspector = client
            .get_feature_inspector("dialog-appearance".to_string())
            .unwrap();

        let s = r#"{
            "positive": { "yes" : { "trait": 1 }  }
        }"#;
        let err1 = inspector
            .get_first_error(s.to_string())
            .unwrap_or_else(|| unreachable!("No error for \"{s}\""));

        let err2 = inspector
            .get_first_error(s.to_string())
            .unwrap_or_else(|| unreachable!("No error for \"{s}\""));

        assert_eq!(err1, err2);

        Ok(())
    }

    #[test]
    fn test_semantic_errors() -> Result<()> {
        let client = client("./browser.yaml", "release")?;
        let inspector = client
            .get_feature_inspector("nimbus-validation".to_string())
            .unwrap();

        let do_test = |lines: &[&str], token: &str, expected: (u32, u32)| {
            let input = lines.join("\n");
            let err = inspector
                .get_first_error(input.clone())
                .unwrap_or_else(|| unreachable!("No error for \"{input}\""));

            assert_eq!(
                err.highlight,
                Some(token.to_string()),
                "Token {token} not detected in error in {input}"
            );

            let observed = (err.line, err.col);
            assert_eq!(
                expected, observed,
                "Error at {token} in the wrong place in {input}"
            );
        };

        // invalid property name.
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,              // 0
                r#"  "invalid": 1"#, // 1
                r#"}"#,              // 2
            ],
            "\"invalid\"",
            (1, 2),
        );

        // simple type mismatch
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                // 0
                r#"  "icon-type": 1"#, // 1
                r#"}"#,                // 2
            ],
            "1",
            (1, 15),
        );

        // enum mismatch
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                        // 0
                r#"  "icon-type": "invalid""#, // 1
                r#"}"#,                        // 2
            ],
            "\"invalid\"",
            (1, 15),
        );

        // invalid field within object
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                   // 0
                r#"  "nested": {"#,       // 1
                r#"    "invalid": true"#, // 2
                r#"  }"#,                 // 3
                r#"}"#,                   // 4
            ],
            "\"invalid\"",
            (2, 4),
        );

        // nested in an object type mismatch
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                    // 0
                r#"  "nested": {"#,        // 1
                r#"    "is-useful": 256"#, // 2
                r#"  }"#,                  // 3
                r#"}"#,                    // 4
            ],
            "256",
            (2, 17),
        );

        // nested in a map type mismatch
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                      // 0
                r#"  "string-int-map": {"#,  // 1
                r#"    "valid": "invalid""#, // 2
                r#"  }"#,                    // 3
                r#"}"#,                      // 4
            ],
            "\"invalid\"",
            (2, 13),
        );

        // invalid key in enum map
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                 // 0
                r#"  "enum-map": {"#,   // 1
                r#"    "invalid": 42"#, // 2
                r#"  }"#,               // 3
                r#"}"#,                 // 4
            ],
            "\"invalid\"",
            (2, 4),
        );

        // type mismatch in list
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                         // 0
                r#"  "nested-list": ["#,        // 1
                r#"     {"#,                    // 2
                r#"        "is-useful": true"#, // 3
                r#"     },"#,                   // 4
                r#"     false"#,                // 5
                r#"  ]"#,                       // 6
                r#"}"#,                         // 7
            ],
            "false",
            (5, 5),
        );

        // Difficult!
        do_test(
            &[
                // 012345678901234567890
                r#"{"#,                          // 0
                r#"  "string-int-map": {"#,      // 1
                r#"    "nested": 1,"#,           // 2
                r#"    "is-useful": 2,"#,        // 3
                r#"    "invalid": 3"#,           // 4 error is not here!
                r#"  },"#,                       // 5
                r#"  "nested": {"#,              // 6
                r#"    "is-useful": "invalid""#, // 7 error is here!
                r#"  }"#,                        // 8
                r#"}"#,                          // 9
            ],
            "\"invalid\"",
            (7, 17),
        );

        Ok(())
    }
}
