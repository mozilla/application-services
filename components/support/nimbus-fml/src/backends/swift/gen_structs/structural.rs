/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt::Display;

use super::common::{self, code_type};
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
    fn property_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
        default: &dyn Display,
    ) -> String {
        // all getters are optional.
        oracle
            .find(&self.inner)
            .property_getter(oracle, vars, prop, default)
    }

    fn value_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        code_type::value_getter(self, oracle, vars, prop)
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn create_transform(&self, oracle: &dyn CodeOracle) -> Option<String> {
        oracle.find(&self.inner).create_transform(oracle)
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
            serde_json::Value::Null => "nil".to_string(),
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
    fn property_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
        default: &dyn Display,
    ) -> String {
        code_type::property_getter(self, oracle, vars, prop, default)
    }

    /// The language specific label used to reference this type. This will be used in
    /// method signatures and property declarations.
    fn type_label(&self, oracle: &dyn CodeOracle) -> String {
        format!(
            "[{k}:{v}]",
            k = oracle.find(&self.k_type).type_label(oracle),
            v = oracle.find(&self.v_type).type_label(oracle),
        )
    }

    fn value_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        let v_type = oracle.find(&self.v_type);
        format!(
            "{vars}?.get{vt}Map({prop})",
            vars = vars,
            vt = v_type.variables_type(oracle),
            prop = common::quoted(prop),
        )
    }

    fn value_mapper(&self, oracle: &dyn CodeOracle) -> Option<String> {
        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);
        Some(
            match (
                k_type.create_transform(oracle),
                v_type.create_transform(oracle),
            ) {
                (Some(k), Some(v)) => format!("mapNotNull({k}, {v})", k = k, v = v),
                (None, Some(v)) =>  format!("mapValues({{{v}}})", v = v),
                // We could do something with keys, but it's only every strings and enums.
                (Some(k), None) => format!("mapKeys({k})", k = k),
                _ => return None,
            },
        )
    }

    fn value_merger(&self, oracle: &dyn CodeOracle, default: &dyn Display) -> Option<String> {
        let v_type = oracle.find(&self.v_type);
        Some(match v_type.merge_transform(oracle) {
            Some(transform) => format!(
                "mergeWith({default}, {transform})",
                default = default,
                transform = transform
            ),
            None => format!("mergeWith({})", default),
        })
    }

    fn create_transform(&self, oracle: &dyn CodeOracle) -> Option<String> {
        let vtype = oracle.find(&self.v_type).variables_type(oracle);

        self.value_mapper(oracle)
            .map(|mapper| {
                format!(
                    r#"{{ (_vars) in return _vars.as{vtype}Map()?.{mapper} }}"#,
                    vtype = vtype,
                    mapper = mapper
                )
            })
            .or_else(|| {
                Some(format!(
                    r#"{{ (_vars) in return _vars.as{vtype}Map()? }}"#,
                    vtype = vtype
                ))
            })
    }

    fn merge_transform(&self, oracle: &dyn CodeOracle) -> Option<String> {
        let overrides = "_overrides";
        let defaults = "_defaults";

        self.value_merger(oracle, &defaults).map(|merger| {
            format!(
                r#"{{ ({overrides}, {defaults}) in return {overrides}.{merger} }}"#,
                overrides = overrides,
                defaults = defaults,
                merger = merger
            )
        })
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        VariablesType::Variables
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
        let k_type = oracle.find(&self.k_type);
        let v_type = oracle.find(&self.v_type);
        let src: Vec<String> = variant
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}: {v}",
                    k = k_type.literal(oracle, renderer, &Literal::String(k.clone())),
                    v = v_type.literal(oracle, renderer, v)
                )
            })
            .collect();

        format!("[{}]", src.join(", "))
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
            "[{item}]",
            item = oracle.find(&self.inner).type_label(oracle),
        )
    }

    fn property_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
        default: &dyn Display,
    ) -> String {
        code_type::property_getter(self, oracle, vars, prop, default)
    }

    fn value_getter(
        &self,
        oracle: &dyn CodeOracle,
        vars: &dyn Display,
        prop: &dyn Display,
    ) -> String {
        let vtype = oracle.find(&self.inner).variables_type(oracle);
        format!(
            "{vars}?.get{vt}List(\"{prop}\")",
            vars = vars,
            vt = vtype,
            prop = prop
        )
    }

    fn value_mapper(&self, oracle: &dyn CodeOracle) -> Option<String> {
        let transform = oracle.find(&self.inner).create_transform(oracle)?;
        Some(if transform.starts_with('{') {
            format!("compactMap {}", transform)
        } else {
            format!("compactMap({})", transform)
        })
    }

    fn value_merger(&self, _oracle: &dyn CodeOracle, _default: &dyn Display) -> Option<String> {
        // We never merge lists.
        None
    }

    /// The name of the type as it's represented in the `Variables` object.
    /// The string return may be used to combine with an indentifier, e.g. a `Variables` method name.
    fn variables_type(&self, _oracle: &dyn CodeOracle) -> VariablesType {
        // Our current implementation of Variables doesn't have a getListList() or getListMap().
        // We do allow getVariablesList and getVariablesMap, but not an vars.asList().
        unimplemented!("Lists and maps of lists aren't supported. The workaround is to use a list of map of list holder objects")
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

        let v_type = oracle.find(&self.inner);
        let src: Vec<String> = variant
            .iter()
            .map(|v| v_type.literal(oracle, renderer, v))
            .collect();

        format!("[{}]", src.join(", "))
    }
}

#[cfg(test)]
mod unit_tests {

    use serde_json::json;

    use crate::backends::swift::gen_structs::{
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
        ct.property_getter(oracle, vars, prop, def)
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
            r#"v?.getStringList("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );

        let ct = list_type("AnObject");
        assert_eq!(
            r#"v?.getVariablesList("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );

        let ct = list_type("String");
        assert_eq!(
            r#"v?.getStringList("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );
    }

    #[test]
    fn test_list_getter_with_fallback() {
        let ct = list_type("String");
        assert_eq!(
            r#"vars?.getStringList("the-property") ?: default"#.to_string(),
            getter_with_fallback(&*ct, &"vars", &"the-property", &"default")
        );

        let ct = list_type("AnEnum");
        assert_eq!(
            r#"vars?.getStringList("the-property")?.mapNotNull(AnEnum::enumValue) ?: default"#
                .to_string(),
            getter_with_fallback(&*ct, &"vars", &"the-property", &"default")
        );

        let ct = list_type("AnObject");
        assert_eq!(
            r#"vars?.getVariablesList("the-property")?.mapNotNull(AnObject::create) ?: default"#
                .to_string(),
            getter_with_fallback(&*ct, &"vars", &"the-property", &"default")
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
            r#"v?.getStringMap("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );

        let ct = map_type("AnEnum", "String");
        assert_eq!(
            r#"v?.getStringMap("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );

        let ct = map_type("AnEnum", "Another");
        assert_eq!(
            r#"v?.getStringMap("the-property")"#.to_string(),
            ct.value_getter(oracle, &"v", &"the-property")
        );
    }

    #[test]
    fn test_map_getter_with_fallback() {
        let oracle = &*oracle();

        let ct = map_type("String", "AnEnum");
        assert_eq!(
             r#"v?.getStringMap("the-property")?.mapValues(AnEnum::enumValue)?.mergeWith(def) ?: def"#.to_string(),
             ct.property_getter(oracle, &"v", &"the-property", &"def")
         );

        let ct = map_type("AnEnum", "String");
        assert_eq!(
            r#"v?.getStringMap("the-property")?.mapKeys(AnEnum::enumValue)?.mergeWith(def) ?: def"#
                .to_string(),
            ct.property_getter(oracle, &"v", &"the-property", &"def")
        );

        let ct = map_type("AnEnum", "Another");
        assert_eq!(
             r#"v?.getStringMap("the-property")?.mapEntries(AnEnum::enumValue, Another::enumValue)?.mergeWith(def) ?: def"#
                 .to_string(),
             ct.property_getter(oracle, &"v", &"the-property", &"def")
         );

        let ct = map_type("AnEnum", "AnObject");
        assert_eq!(
             r#"v?.getVariablesMap("the-property")?.mapEntries(AnEnum::enumValue, AnObject::create)?.mergeWith(def, AnObject::mergeWith) ?: def"#.to_string(),
             ct.property_getter(oracle, &"v", &"the-property", &"def"));
    }
}
