/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use crate::backends::{CodeOracle, CodeType, LiteralRenderer, VariablesType};
use crate::intermediate_representation::Literal;

pub(crate) struct BooleanCodeType;

impl CodeType for BooleanCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        "Boolean".into()
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::Bool
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides} ?: {default}",
            overrides = overrides,
            default = default
        )
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        _oracle: &dyn CodeOracle,
        _renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        match literal {
            serde_json::Value::Bool(v) => {
                if *v {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            _ => unreachable!("Expecting a boolean"),
        }
    }
}

pub(crate) struct IntCodeType;

impl CodeType for IntCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        "Int".into()
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::Int
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides} ?: {default}",
            overrides = overrides,
            default = default
        )
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        _oracle: &dyn CodeOracle,
        _renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        match literal {
            serde_json::Value::Number(v) => {
                format!("{:.0}", v)
            }
            _ => unreachable!("Expecting a number"),
        }
    }
}

pub(crate) struct StringCodeType;

impl CodeType for StringCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        "String".into()
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::String
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides} ?: {default}",
            overrides = overrides,
            default = default
        )
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        _oracle: &dyn CodeOracle,
        _renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        match literal {
            serde_json::Value::String(v) => {
                // Usually, we'd be wanting to escape this, for security reasons. However, this is
                // will cause a kotlinc compile time error when the app is built if the string is malformed
                // in the manifest.
                format!(r#""{}""#, v)
            }
            _ => unreachable!("Expecting a string"),
        }
    }
}

#[cfg(test)]
mod unit_tests {

    use serde_json::json;

    use crate::backends::TypeIdentifier;

    use super::*;

    struct TestCodeOracle;
    impl CodeOracle for TestCodeOracle {
        fn find(&self, _type_: &TypeIdentifier) -> Box<dyn CodeType> {
            unreachable!()
        }
    }

    struct TestRenderer;
    impl LiteralRenderer for TestRenderer {
        fn literal(
            &self,
            _oracle: &dyn CodeOracle,
            _typ: &TypeIdentifier,
            _value: &Literal,
        ) -> String {
            unreachable!()
        }
    }

    fn oracle() -> Box<dyn CodeOracle> {
        Box::new(TestCodeOracle) as Box<dyn CodeOracle>
    }

    fn bool_type() -> Box<dyn CodeType> {
        Box::new(BooleanCodeType) as Box<dyn CodeType>
    }

    fn string_type() -> Box<dyn CodeType> {
        Box::new(StringCodeType) as Box<dyn CodeType>
    }

    fn int_type() -> Box<dyn CodeType> {
        Box::new(IntCodeType) as Box<dyn CodeType>
    }

    #[test]
    fn test_type_label() {
        let oracle = &*oracle();

        let ct = bool_type();
        assert_eq!("Boolean".to_string(), ct.type_label(oracle));

        let ct = string_type();
        assert_eq!("String".to_string(), ct.type_label(oracle));

        let ct = int_type();
        assert_eq!("Int".to_string(), ct.type_label(oracle));
    }

    #[test]
    fn test_literal() {
        let oracle = &*oracle();
        let finder = &TestRenderer;

        let ct = bool_type();
        assert_eq!("true".to_string(), ct.literal(oracle, finder, &json!(true)));
        assert_eq!(
            "false".to_string(),
            ct.literal(oracle, finder, &json!(false))
        );

        let ct = string_type();
        assert_eq!(
            r#""no""#.to_string(),
            ct.literal(oracle, finder, &json!("no"))
        );
        assert_eq!(
            r#""yes""#.to_string(),
            ct.literal(oracle, finder, &json!("yes"))
        );

        let ct = int_type();
        assert_eq!("1".to_string(), ct.literal(oracle, finder, &json!(1)));
        assert_eq!("2".to_string(), ct.literal(oracle, finder, &json!(2)));
    }

    #[test]
    fn test_get_value() {
        let oracle = &*oracle();

        let ct = bool_type();
        assert_eq!(
            r#"v?.getBool("the-property")"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = string_type();
        assert_eq!(
            r#"v?.getString("the-property")"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = int_type();
        assert_eq!(
            r#"v?.getInt("the-property")"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );
    }

    #[test]
    fn test_with_fallback() {
        let oracle = &*oracle();

        let ct = bool_type();
        assert_eq!(
            "value ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );

        let ct = string_type();
        assert_eq!(
            "value ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );

        let ct = int_type();
        assert_eq!(
            "value ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );
    }
}
