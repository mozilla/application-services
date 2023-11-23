// /* This Source Code Form is subject to the terms of the Mozilla Public
//  * License, v. 2.0. If a copy of the MPL was not distributed with this
//  * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::{common, ConcreteCodeOracle};
use std::borrow::Borrow;
use std::fmt::{self, Display};

use crate::backends::{CodeOracle, LiteralRenderer, TypeIdentifier};
use crate::intermediate_representation::Literal;

pub fn type_label(type_: impl Borrow<TypeIdentifier>) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_.borrow()).type_label(&oracle))
}

pub fn defaults_type_label(type_: impl Borrow<TypeIdentifier>) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle.find(type_.borrow()).defaults_type(&oracle))
}

pub fn literal(
    type_: impl Borrow<TypeIdentifier>,
    renderer: impl LiteralRenderer,
    literal: impl Borrow<Literal>,
    ctx: impl Display,
) -> Result<String, askama::Error> {
    let oracle = ConcreteCodeOracle;
    Ok(oracle
        .find(type_.borrow())
        .literal(&oracle, &ctx, &renderer, literal.borrow()))
}

pub fn property(
    type_: impl Borrow<TypeIdentifier>,
    prop: impl fmt::Display,
    vars: impl fmt::Display,
    default: impl fmt::Display,
) -> Result<String, askama::Error> {
    let oracle = &ConcreteCodeOracle;
    let ct = oracle.find(type_.borrow());
    Ok(ct.property_getter(oracle, &vars, &prop, &default))
}

pub fn to_json(
    prop: impl fmt::Display,
    type_: impl Borrow<TypeIdentifier>,
) -> Result<String, askama::Error> {
    let oracle = &ConcreteCodeOracle;
    let ct = oracle.find(type_.borrow());
    Ok(ct.as_json(oracle, &prop))
}

/// Get the idiomatic Swift rendering of a class name (for enums, records, errors, etc).
pub fn class_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::class_name(&nm))
}

/// Get the idiomatic Swift rendering of a variable name.
pub fn var_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::var_name(&nm))
}

/// Get the idiomatic Swift rendering of an individual enum variant.
pub fn enum_variant_name(nm: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::enum_variant_name(&nm))
}

pub fn comment(txt: impl fmt::Display, spaces: &str) -> Result<String, askama::Error> {
    use textwrap::{fill, Options};

    let indent1 = "/// ".to_string();
    let indent2 = format!("{} /// ", spaces);

    let options = Options::new(80)
        .initial_indent(&indent1)
        .subsequent_indent(&indent2);

    let lines = fill(txt.to_string().as_str(), options);
    Ok(lines)
}

pub fn quoted(txt: impl fmt::Display) -> Result<String, askama::Error> {
    Ok(common::quoted(&txt))
}

#[cfg(test)]
mod json_tests {
    use crate::intermediate_representation::TypeRef;

    use super::*;
    use askama::Error;

    #[test]
    fn scalar_types() -> Result<(), Error> {
        let p = "prop";

        assert_eq!(to_json(p, TypeRef::Boolean)?, format!("{p}"));
        assert_eq!(to_json(p, TypeRef::Int)?, format!("{p}"));
        assert_eq!(to_json(p, TypeRef::String)?, format!("{p}"));
        assert_eq!(
            to_json(p, TypeRef::StringAlias("Name".to_string()))?,
            format!("{p}")
        );
        assert_eq!(
            to_json(p, TypeRef::Enum("Name".to_string()))?,
            format!("{p}.rawValue")
        );
        Ok(())
    }

    #[test]
    fn bundled_types() -> Result<(), Error> {
        let p = "prop";
        assert_eq!(to_json(p, TypeRef::BundleText)?, format!("{p}"));
        assert_eq!(
            to_json(p, TypeRef::BundleImage)?,
            format!("{p}.encodableImageName")
        );

        Ok(())
    }

    #[test]
    fn optional_types() -> Result<(), Error> {
        let p = "prop";
        assert_eq!(
            to_json(p, TypeRef::Option(Box::new(TypeRef::String)))?,
            format!("{p}")
        );
        assert_eq!(
            to_json(p, TypeRef::Option(Box::new(TypeRef::BundleImage)))?,
            format!("{p}?.encodableImageName")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::Option(Box::new(TypeRef::Enum("Name".to_string())))
            )?,
            format!("{p}?.rawValue")
        );
        Ok(())
    }

    #[test]
    fn list_types() -> Result<(), Error> {
        let p = "prop";
        assert_eq!(
            to_json(p, TypeRef::List(Box::new(TypeRef::String)))?,
            format!("{p}")
        );
        assert_eq!(
            to_json(p, TypeRef::List(Box::new(TypeRef::BundleImage)))?,
            format!("{p}.map {{ $0.encodableImageName }}")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::List(Box::new(TypeRef::Enum("Name".to_string())))
            )?,
            format!("{p}.map {{ $0.rawValue }}")
        );
        Ok(())
    }

    #[test]
    fn string_map_types() -> Result<(), Error> {
        let p = "prop";
        assert_eq!(
            to_json(p, TypeRef::StringMap(Box::new(TypeRef::String)))?,
            format!("{p}")
        );
        assert_eq!(
            to_json(p, TypeRef::StringMap(Box::new(TypeRef::BundleImage)))?,
            format!("{p}.mapValuesNotNull {{ $0.encodableImageName }}")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::StringMap(Box::new(TypeRef::Enum("Name".to_string())))
            )?,
            format!("{p}.mapValuesNotNull {{ $0.rawValue }}")
        );

        assert_eq!(
            to_json(
                p,
                TypeRef::StringMap(Box::new(TypeRef::Option(Box::new(TypeRef::String))))
            )?,
            format!("{p}")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::StringMap(Box::new(TypeRef::Option(Box::new(TypeRef::BundleImage))))
            )?,
            format!("{p}.mapValuesNotNull {{ $0?.encodableImageName }}")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::StringMap(Box::new(TypeRef::Option(Box::new(TypeRef::Enum(
                    "Name".to_string()
                )))))
            )?,
            format!("{p}.mapValuesNotNull {{ $0?.rawValue }}")
        );
        Ok(())
    }

    #[test]
    fn enum_map_types_keys() -> Result<(), Error> {
        let p = "prop";
        assert_eq!(
            to_json(
                p,
                TypeRef::EnumMap(Box::new(TypeRef::String), Box::new(TypeRef::String))
            )?,
            format!("{p}")
        );
        assert_eq!(
            to_json(
                p,
                TypeRef::EnumMap(
                    Box::new(TypeRef::StringAlias("Name".to_string())),
                    Box::new(TypeRef::String)
                )
            )?,
            format!("{p}")
        );

        // Mapping keys for enums. We do this because Swift encodes Dictionary<Enum, T> as
        // an array: [k0, v0, k1, v1]. Bizarre!
        assert_eq!(
            to_json(
                p,
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("Name".to_string())),
                    Box::new(TypeRef::String)
                )
            )?,
            format!("{p}.mapKeysNotNull {{ $0.rawValue }}")
        );
        Ok(())
    }

    #[test]
    fn enum_map_types_keys_and_values() -> Result<(), Error> {
        let p = "prop";

        // Mapping keys for enums. We do this because Swift encodes Dictionary<Enum, T> as
        // an array: [k0, v0, k1, v1]. Bizarre!
        // Map<Enum, Image>
        assert_eq!(
            to_json(
                p,
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("Name".to_string())),
                    Box::new(TypeRef::BundleImage)
                )
            )?,
            format!("prop.mapEntriesNotNull({{ $0.rawValue }}, {{ $0.encodableImageName }})")
        );

        // Map<Enum, List<String>>; we don't need to map the values, because they encode cleanly.
        assert_eq!(
            to_json(
                p,
                TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("Name".to_string())),
                    Box::new(TypeRef::List(Box::new(TypeRef::String)))
                )
            )?,
            format!("prop.mapKeysNotNull {{ $0.rawValue }}")
        );

        // Map<Enum, List<Image>>
        assert_eq!(
            to_json(p, TypeRef::EnumMap(Box::new(TypeRef::Enum("Name".to_string())), Box::new(TypeRef::List(Box::new(TypeRef::BundleImage)))))?,
            format!("prop.mapEntriesNotNull({{ $0.rawValue }}, {{ $0.map {{ $0.encodableImageName }} }})")
        );
        Ok(())
    }
}
