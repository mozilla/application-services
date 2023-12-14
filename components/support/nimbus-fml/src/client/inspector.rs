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

    pub fn get_errors(&self, string: String) -> Option<Vec<FmlEditorError>> {
        match self.parse_json_string(&string) {
            Err(e) => Some(vec![e]),
            Ok(json) => {
                let errors = self.get_semantic_errors(&string, json);
                if errors.is_empty() {
                    None
                } else {
                    Some(errors)
                }
            }
        }
    }

    pub fn get_schema_hash(&self) -> String {
        self.manifest
            .get_schema_hash(&self.feature_id)
            .unwrap_or_default()
    }

    pub fn get_defaults_hash(&self) -> String {
        self.manifest
            .get_defaults_hash(&self.feature_id)
            .unwrap_or_default()
    }
}

impl FmlFeatureInspector {
    fn parse_json_string(&self, string: &str) -> Result<Value, FmlEditorError> {
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

    fn get_semantic_errors(&self, src: &str, value: Value) -> Vec<FmlEditorError> {
        let errors = self
            .manifest
            .get_errors(&self.feature_id, &value)
            .unwrap_or_else(|e| {
                unreachable!("Error {e:?} should be caught as FeatureValidationError")
            });
        let mut editor_errors: Vec<_> = Vec::with_capacity(errors.len());
        for e in errors {
            let message = e.message;
            let highlight = e.path.last_token().map(str::to_string);
            let (line, col) = e.path.line_col(src);
            let error = FmlEditorError {
                message,
                line: line as u32,
                col: col as u32,
                highlight,
            };
            editor_errors.push(error);
        }
        editor_errors
    }
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

    impl FmlFeatureInspector {
        fn get_first_error(&self, string: String) -> Option<FmlEditorError> {
            let mut errors = self.get_errors(string)?;
            errors.pop()
        }
    }

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
