/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use askama::Template;
use std::fmt::Display;

use crate::backends::{CodeDeclaration, CodeOracle, CodeType, VariablesType};
use crate::intermediate_representation::{self, FeatureManifest, ObjectDef};

use super::filters;

use super::{identifiers, ConcreteCodeOracle};

pub struct ObjectRuntime;

impl CodeDeclaration for ObjectRuntime {}

pub struct ObjectCodeType {
    id: String,
}

impl ObjectCodeType {
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

impl CodeType for ObjectCodeType {
    fn type_label(&self, _oracle: &dyn CodeOracle) -> String {
        identifiers::class_name(&self.id)
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(
        &self,
        _oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        format!(
            "{vars}.getVariables({prop})",
            vars = vars,
            // transform = self.transform(oracle).unwrap(),
            prop = identifiers::quoted(prop)
        )
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::Variables
    }

    fn transform(&self, oracle: &dyn CodeOracle) -> Option<String> {
        Some(format!("{}::create", self.type_label(oracle)))
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        format!(
            "{overrides}?.let {{ {t}(it, {default}._defaults) }} ?: {default}",
            t = self.type_label(oracle),
            overrides = overrides,
            default = default
        )
    }

    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        literal: &intermediate_representation::Literal,
    ) -> String {
        match literal {
            serde_json::Value::Object(map) => {
                if map.is_empty() {
                    format!("{}()", self.id)
                } else {
                    // https://mozilla-hub.atlassian.net/browse/SDK-433
                    unimplemented!("SDK-433: Object literals are not yet implemented")
                }
            }
            _ => unreachable!(
                "An JSON object is expected for {} object literal",
                self.type_label(oracle)
            ),
        }
    }
}

#[derive(Template)]
#[template(syntax = "kt", escape = "none", path = "ObjectTemplate.kt")]
pub(crate) struct ObjectCodeDeclaration {
    inner: ObjectDef,
    _oracle: ConcreteCodeOracle,
}

impl ObjectCodeDeclaration {
    pub fn new(_fm: &FeatureManifest, inner: &ObjectDef) -> Self {
        Self {
            _oracle: Default::default(),
            inner: inner.clone(),
        }
    }
    pub fn inner(&self) -> ObjectDef {
        self.inner.clone()
    }
}

impl CodeDeclaration for ObjectCodeDeclaration {
    fn definition_code(&self, _oracle: &dyn CodeOracle) -> Option<String> {
        Some(self.render().unwrap())
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

    fn oracle() -> Box<dyn CodeOracle> {
        Box::new(TestCodeOracle) as Box<dyn CodeOracle>
    }

    fn code_type(name: &str) -> Box<dyn CodeType> {
        Box::new(ObjectCodeType::new(name.to_string())) as Box<dyn CodeType>
    }

    fn getter_with_fallback(
        ct: &dyn CodeType,
        vars: &dyn Display,
        prop: &dyn Display,
        def: &dyn Display,
    ) -> String {
        let oracle = &*oracle();
        let getter = ct.get_value(oracle, vars, prop);
        ct.with_fallback(oracle, &getter, def)
    }

    #[test]
    fn test_type_label() {
        let ct = code_type("AnObject");
        let oracle = &*oracle();
        assert_eq!("AnObject".to_string(), ct.type_label(oracle))
    }

    #[test]
    fn test_literal() {
        let ct = code_type("AnObject");
        let oracle = &*oracle();
        assert_eq!("AnObject()".to_string(), ct.literal(oracle, &json!({})));
    }

    #[test]
    fn test_get_value() {
        let ct = code_type("AnObject");
        let oracle = &*oracle();

        assert_eq!(
            r#"v?.getVariables("the-property")"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );
    }

    #[test]
    fn test_with_fallback() {
        let ct = code_type("AnObject");
        let oracle = &*oracle();

        assert_eq!(
            "vars?.let { AnObject(it, default._defaults) } ?: default".to_string(),
            ct.with_fallback(oracle, &"vars", &"default")
        );
    }

    #[test]
    fn test_getter_with_fallback() {
        let ct = code_type("AnObject");
        assert_eq!(
            r#"vars?.getVariables("the-property")?.let { AnObject(it, default._defaults) } ?: default"#
            .to_string(),
            getter_with_fallback(&*ct, &"vars?", &"the-property", &"default"));
    }
}
