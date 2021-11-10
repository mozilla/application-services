/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use super::identifiers;
use crate::backends::{LiteralRenderer, VariablesType};
use crate::{
    backends::{CodeOracle, CodeType, TypeIdentifier},
    intermediate_representation::Literal,
};

pub(crate) struct OptionalCodeType {
    inner: TypeIdentifier,
}

impl OptionalCodeType {
    pub(crate) fn new(inner: &TypeIdentifier) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
}

impl CodeType for OptionalCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "{item}?",
            item = oracle.find(&self.inner).type_label(oracle),
        )
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        // all getters are optional.
        oracle.find(&self.inner).get_value(oracle, vars, prop)
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

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, oracle: &dyn CodeOracle) -> VariablesType {
        oracle.find(&self.inner).variables_type(oracle)
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        match literal {
            serde_json::Value::Null => "null".to_string(),
            _ => oracle.find(&self.inner).literal(oracle, renderer, literal),
        }
    }
}

// Map type

pub(crate) struct MapCodeType {
    k_type: TypeIdentifier,
    v_type: TypeIdentifier,
}

impl MapCodeType {
    pub(crate) fn new(k: &TypeIdentifier, v: &TypeIdentifier) -> Self {
        Self {
            k_type: k.clone(),
            v_type: v.clone(),
        }
    }
}

impl CodeType for MapCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "Map<{k}, {v}>",
            k = oracle.find(&self.k_type).type_label(oracle),
            v = oracle.find(&self.v_type).type_label(oracle),
        )
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);

        let getter = format!(
            "{vars}.get{vt}Map({prop})",
            vars = vars,
            vt = v_type.variables_type(oracle),
            prop = identifiers::quoted(prop),
        );

        let mapper = match (k_type.transform(oracle), v_type.transform(oracle)) {
            (Some(k), Some(v)) => format!("?.mapEntries({k}, {v})", k = k, v = v),
            (None, Some(v)) => format!("?.mapValues({v})", v = v),
            (Some(k), None) => format!("?.mapKeys({k})", k = k),
            _ => "".into(),
        };

        format!("{}{}", getter, mapper)
    }

    /// Accepts two runtime expressions and returns a runtime experession to combine. If the `default` is of type `T`,
    /// the `override` is of type `T?`.
    fn with_fallback(
        &self,
        _oracle: &dyn CodeOracle,
        overrides: &dyn Display,
        default: &dyn Display,
    ) -> String {
        match &self.v_type {
            // https://mozilla-hub.atlassian.net/browse/SDK-435
            TypeIdentifier::Object(..) => {
                unimplemented!("SDK-435 Cannot yet merge maps of objects")
            }

            // https://mozilla-hub.atlassian.net/browse/SDK-435
            TypeIdentifier::EnumMap(..) | TypeIdentifier::StringMap(..) => {
                unimplemented!("SDK-435 Cannot yet merge maps of maps")
            }

            _ => format!(
                "{overrides}?.let {{ overrides -> {default} + overrides }} ?: {default}",
                overrides = overrides,
                default = default
            ),
        }
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        unimplemented!("Nesting maps in to lists and maps are not supported")
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        let variant = match literal {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        };

        if let TypeIdentifier::Object(..) = &self.v_type {
            // https://mozilla-hub.atlassian.net/browse/SDK-434
            unimplemented!("SDK-434 Cannot render object literals in maps")
        };

        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);
        let src: Vec<String> = variant
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k} to {v}",
                    k = k_type.literal(oracle, renderer, &Literal::String(k.clone())),
                    v = v_type.literal(oracle, renderer, v)
                )
            })
            .collect();

        format!("mapOf({})", src.join(", "))
    }
}

// List type

pub(crate) struct ListCodeType {
    inner: TypeIdentifier,
}

impl ListCodeType {
    pub(crate) fn new(inner: &TypeIdentifier) -> Self {
        Self {
            inner: inner.clone(),
        }
    }
}

impl CodeType for ListCodeType {
    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "List<{item}>",
            item = oracle.find(&self.inner).type_label(oracle),
        )
    }

    /// The language specific expression that gets a value of the `prop` from the `vars` object.
    fn get_value(&self, oracle: &dyn CodeOracle, vars: &dyn Display, prop: &dyn Display) -> String {
        let v_type = oracle.find(&self.inner);

        let getter = format!(
            "{vars}.get{vt}List({prop})",
            vars = vars,
            vt = v_type.variables_type(oracle),
            prop = identifiers::quoted(prop),
        );

        let mapper = match v_type.transform(oracle) {
            Some(item) => format!("?.mapNotNull({item})", item = item),
            _ => "".into(),
        };

        format!("{}{}", getter, mapper)
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

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        unimplemented!("Nesting lists in to lists and maps are not supported")
    }

    /// A representation of the given literal for this type.
    /// N.B. `Literal` is aliased from `serde_json::Value`.
    fn literal(
        &self,
        oracle: &dyn CodeOracle,
        renderer: &dyn LiteralRenderer,
        literal: &Literal,
    ) -> String {
        let variant = match literal {
            serde_json::Value::Array(v) => v,
            _ => unreachable!(),
        };

        if let TypeIdentifier::Object(..) = &self.inner {
            // https://mozilla-hub.atlassian.net/browse/SDK-434
            unimplemented!("SDK-434 Cannot render object literals in lists")
        }

        let v_type = oracle.find(&self.inner);
        let src: Vec<String> = variant
            .iter()
            .map(|v| v_type.literal(oracle, renderer, v))
            .collect();

        format!("listOf({})", src.join(", "))
    }
}

#[cfg(test)]
mod unit_tests {

    use serde_json::json;

    use crate::backends::kotlin::gen_structs::{
        enum_::EnumCodeType, object::ObjectCodeType, primitives::StringCodeType,
    };
    use crate::backends::TypeIdentifier;

    use super::*;

    struct TestCodeOracle;
    impl CodeOracle for TestCodeOracle {
        fn find(&self, type_: &TypeIdentifier) -> Box<dyn CodeType> {
            match type_ {
                TypeIdentifier::String => Box::new(StringCodeType) as Box<dyn CodeType>,
                TypeIdentifier::Enum(s) => {
                    Box::new(EnumCodeType::new(s.clone())) as Box<dyn CodeType>
                }
                TypeIdentifier::Object(s) => {
                    Box::new(ObjectCodeType::new(s.clone())) as Box<dyn CodeType>
                }
                TypeIdentifier::List(i) => Box::new(ListCodeType::new(i)),
                TypeIdentifier::EnumMap(k, v) => Box::new(MapCodeType::new(k, v)),
                _ => unreachable!(),
            }
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

    fn type_(nm: &str) -> TypeIdentifier {
        match nm {
            "String" => TypeIdentifier::String,
            "AnObject" => TypeIdentifier::Object("AnObject".to_string()),
            nm => TypeIdentifier::Enum(nm.to_string()),
        }
    }

    fn list_type(item: &str) -> Box<dyn CodeType> {
        Box::new(ListCodeType::new(&type_(item)))
    }

    fn map_type(k: &str, v: &str) -> Box<dyn CodeType> {
        Box::new(MapCodeType::new(&type_(k), &type_(v)))
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
    fn test_list_type_label() {
        let oracle = &*oracle();
        let ct = list_type("String");
        assert_eq!("List<String>".to_string(), ct.type_label(oracle));

        let ct = list_type("AnEnum");
        assert_eq!("List<AnEnum>".to_string(), ct.type_label(oracle));
    }

    #[test]
    fn test_list_literal() {
        let oracle = &*oracle();
        let finder = &TestRenderer;

        let ct = list_type("String");
        assert_eq!(
            r#"listOf("x", "y", "z")"#.to_string(),
            ct.literal(oracle, finder, &json!(["x", "y", "z"]))
        );

        let ct = list_type("AnEnum");
        assert_eq!(
            r#"listOf(AnEnum.X, AnEnum.Y, AnEnum.Z)"#.to_string(),
            ct.literal(oracle, finder, &json!(["x", "y", "z"]))
        );
    }

    #[test]
    fn test_list_get_value() {
        let oracle = &*oracle();

        let ct = list_type("AnEnum");
        assert_eq!(
            "v?.getStringList(\"the-property\")?.mapNotNull(AnEnum::enumValue)".to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = list_type("AnObject");
        assert_eq!(
            "v?.getVariablesList(\"the-property\")?.mapNotNull(AnObject::create)".to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = list_type("String");
        assert_eq!(
            "v?.getStringList(\"the-property\")".to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );
    }

    #[test]
    fn test_list_with_fallback() {
        let oracle = &*oracle();

        let ct = list_type("AnObject");
        assert_eq!(
            "list ?: default".to_string(),
            ct.with_fallback(oracle, &"list", &"default")
        );

        let ct = list_type("AnEnum");
        assert_eq!(
            "list ?: default".to_string(),
            ct.with_fallback(oracle, &"list", &"default")
        );

        let ct = list_type("String");
        assert_eq!(
            "list ?: default".to_string(),
            ct.with_fallback(oracle, &"list", &"default")
        );
    }

    #[test]
    fn test_list_getter_with_fallback() {
        let ct = list_type("String");
        assert_eq!(
            r#"vars?.getStringList("the-property") ?: default"#.to_string(),
            getter_with_fallback(&*ct, &"vars?", &"the-property", &"default")
        );

        let ct = list_type("AnEnum");
        assert_eq!(
            r#"vars?.getStringList("the-property")?.mapNotNull(AnEnum::enumValue) ?: default"#
                .to_string(),
            getter_with_fallback(&*ct, &"vars?", &"the-property", &"default")
        );

        let ct = list_type("AnObject");
        assert_eq!(
            r#"vars?.getVariablesList("the-property")?.mapNotNull(AnObject::create) ?: default"#
                .to_string(),
            getter_with_fallback(&*ct, &"vars?", &"the-property", &"default")
        );
    }

    #[test]
    fn test_map_type_label() {
        let oracle = &*oracle();
        let ct = map_type("String", "String");
        assert_eq!("Map<String, String>".to_string(), ct.type_label(oracle));

        let ct = map_type("String", "AnEnum");
        assert_eq!("Map<String, AnEnum>".to_string(), ct.type_label(oracle));
    }

    #[test]
    fn test_map_literal() {
        let oracle = &*oracle();
        let finder = &TestRenderer;

        let ct = map_type("String", "AnEnum");
        assert_eq!(
            r#"mapOf("a" to AnEnum.A, "b" to AnEnum.B)"#.to_string(),
            ct.literal(oracle, finder, &json!({"a": "a", "b": "b"}))
        );

        let ct = map_type("AnEnum", "String");
        assert_eq!(
            r#"mapOf(AnEnum.A to "a", AnEnum.B to "b")"#.to_string(),
            ct.literal(oracle, finder, &json!({"a": "a", "b": "b"}))
        );
    }

    #[test]
    fn test_map_get_value() {
        let oracle = &*oracle();

        let ct = map_type("String", "AnEnum");
        assert_eq!(
            r#"v?.getStringMap("the-property")?.mapValues(AnEnum::enumValue)"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = map_type("AnEnum", "String");
        assert_eq!(
            r#"v?.getStringMap("the-property")?.mapKeys(AnEnum::enumValue)"#.to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = map_type("AnEnum", "Another");
        assert_eq!(
            r#"v?.getStringMap("the-property")?.mapEntries(AnEnum::enumValue, Another::enumValue)"#
                .to_string(),
            ct.get_value(oracle, &"v?", &"the-property")
        );

        let ct = map_type("AnEnum", "AnObject");
        assert_eq!(r#"v?.getVariablesMap("the-property")?.mapEntries(AnEnum::enumValue, AnObject::create)"#.to_string(), ct.get_value(oracle, &"v?", &"the-property"));
    }

    #[test]
    fn test_map_with_fallback() {
        let oracle = &*oracle();

        let ct = map_type("String", "String");
        assert_eq!(
            "value?.let { overrides -> default + overrides } ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );

        let ct = map_type("AnEnum", "String");
        assert_eq!(
            "value?.let { overrides -> default + overrides } ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );

        let ct = map_type("AnEnum", "Another");
        assert_eq!(
            "value?.let { overrides -> default + overrides } ?: default".to_string(),
            ct.with_fallback(oracle, &"value", &"default")
        );
    }
}
