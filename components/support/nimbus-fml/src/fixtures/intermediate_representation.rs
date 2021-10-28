/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::intermediate_representation::{
    EnumDef, FeatureDef, FeatureManifest, PropDef, TypeRef, VariantDef,
};
use serde_json::json;

pub(crate) fn get_simple_nimbus_validation_feature() -> FeatureManifest {
    FeatureManifest {
        enum_defs: vec![EnumDef {
            name: "Position".into(),
            doc: "Where to put the menu bar?".into(),
            variants: vec![
                VariantDef {
                    name: "top".into(),
                    doc: "The top of the screen".into(),
                },
                VariantDef {
                    name: "bottom".into(),
                    doc: "The bottom of the screen".into(),
                },
            ],
        }],
        obj_defs: Default::default(),
        hints: Default::default(),
        feature_defs: vec![FeatureDef::new(
            "nimbus-validation",
            "A simple validation feature",
            vec![
                PropDef {
                    name: "enabled".into(),
                    doc: "An example boolean property".into(),
                    typ: TypeRef::Boolean,
                    default: json!(true),
                },
                PropDef {
                    name: "row-count".into(),
                    doc: "An example integer property".into(),
                    typ: TypeRef::Int,
                    default: json!(2),
                },
                PropDef {
                    name: "deeplink".into(),
                    doc: "An example string property".into(),
                    typ: TypeRef::String,
                    default: json!("deeplink://settings"),
                },
                PropDef {
                    name: "menu-position".into(),
                    doc: "Where to put the menu".into(),
                    typ: TypeRef::Enum("Position".into()),
                    default: json!("bottom"),
                },
                PropDef {
                    name: "enum-map".into(),
                    doc: "A map of enums to booleans".into(),
                    typ: TypeRef::EnumMap(
                        Box::new(TypeRef::Enum("Position".into())),
                        Box::new(TypeRef::Boolean),
                    ),
                    default: json!({"bottom": true, "top": false}),
                },
                PropDef {
                    name: "string-map".into(),
                    doc: "A map of string to enums".into(),
                    typ: TypeRef::StringMap(Box::new(TypeRef::Enum("Position".into()))),
                    default: json!({"foo": "bottom", "bar": "top"}),
                },
                PropDef {
                    name: "int-list".into(),
                    doc: "A list of numbers".into(),
                    typ: TypeRef::List(Box::new(TypeRef::Int)),
                    default: json!([1, 2, 3]),
                },
                PropDef {
                    name: "enum-list".into(),
                    doc: "A list of enums".into(),
                    typ: TypeRef::List(Box::new(TypeRef::Enum("Position".into()))),
                    default: json!(["top", "bottom"]),
                },
            ],
            None,
        )],
    }
}

pub(crate) fn get_simple_homescreen_feature() -> FeatureManifest {
    FeatureManifest {
        enum_defs: vec![EnumDef {
            name: "SectionId".into(),
            doc: "The sections of the homescreen".into(),
            variants: vec![
                VariantDef::new("top-sites", "The original frecency sorted sites"),
                VariantDef::new("jump-back-in", "Jump back in section"),
                VariantDef::new("recently-saved", "Tabs that have been bookmarked recently"),
            ],
        }],
        obj_defs: Default::default(),
        hints: Default::default(),
        feature_defs: vec![FeatureDef::new(
            "homescreen",
            "Represents the homescreen feature",
            vec![PropDef {
                name: "sections-enabled".into(),
                doc: "A map of booleans".into(),
                typ: TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("SectionId".into())),
                    Box::new(TypeRef::Boolean),
                ),
                default: json!({
                    "top-sites": true,
                    "jump-back-in": false,
                    "recently-saved": false,
                }),
            }],
            None,
        )],
    }
}

#[cfg(test)]
mod dump_to_file {
    use std::path::PathBuf;

    use crate::error::Result;

    use super::*;

    fn write(fm: &FeatureManifest, nm: &str) -> Result<()> {
        let root = std::env::var("CARGO_MANIFEST_DIR")
            .expect("Missing $CARGO_MANIFEST_DIR, cannot write fixtures files");
        let fixtures_dir = "fixtures/ir";
        let path: PathBuf = [&root, fixtures_dir, nm].iter().collect();

        let contents = serde_json::to_string_pretty(fm)?;

        std::fs::write(path, contents)?;

        Ok(())
    }

    #[test]
    fn write_to_fixtures_dir() -> Result<()> {
        write(&get_simple_homescreen_feature(), "simple_homescreen.json")?;
        write(
            &get_simple_nimbus_validation_feature(),
            "simple_nimbus_validation.json",
        )?;

        Ok(())
    }
}
