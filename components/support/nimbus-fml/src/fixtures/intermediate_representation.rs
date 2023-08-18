/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::intermediate_representation::{
    EnumDef, FeatureDef, FeatureManifest, PropDef, TypeRef, VariantDef,
};
use serde_json::json;

pub(crate) fn get_simple_homescreen_feature() -> FeatureManifest {
    FeatureManifest {
        enum_defs: vec![EnumDef {
            name: "HomeScreenSection".into(),
            doc: "The sections of the homescreen".into(),
            variants: vec![
                VariantDef::new("top-sites", "The original frecency sorted sites"),
                VariantDef::new("jump-back-in", "Jump back in section"),
                VariantDef::new("recently-saved", "Tabs that have been bookmarked recently"),
                VariantDef::new("recent-explorations", "Tabs from another source"),
                VariantDef::new("pocket", "Tabs from another source"),
            ],
        }],
        obj_defs: Default::default(),
        feature_defs: vec![FeatureDef::new(
            "homescreen",
            "Represents the homescreen feature",
            vec![PropDef {
                name: "sections-enabled".into(),
                doc: "A map of booleans".into(),
                typ: TypeRef::EnumMap(
                    Box::new(TypeRef::Enum("HomeScreenSection".into())),
                    Box::new(TypeRef::Boolean),
                ),
                default: json!({
                    "top-sites": true,
                    "jump-back-in": false,
                    "recently-saved": false,
                    "recent-explorations": false,
                    "pocket": false,
                }),
            }],
            false,
        )],
        ..Default::default()
    }
}
