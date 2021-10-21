/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::ir::{EnumDef, FeatureDef, FeatureManifest, PropDef, TypeRef, VariantDef};
use serde_json::json;

pub(crate) fn get_simple_nimbus_validation_feature() -> FeatureManifest {
    FeatureManifest {
        enum_defs: vec![],
        obj_defs: vec![],
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
                    typ: TypeRef::Boolean,
                    default: json!(2),
                },
                PropDef {
                    name: "deeplink".into(),
                    doc: "An example string property".into(),
                    typ: TypeRef::String,
                    default: json!("deeplink://settings"),
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
        obj_defs: vec![],
        feature_defs: vec![FeatureDef::new(
            "homescreen",
            "Represents the homescreen feature",
            vec![PropDef {
                name: "sections-enabled".into(),
                doc: "A map of booleans".into(),
                typ: TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("SectionId".into())),
                    Box::new(TypeRef::String),
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
