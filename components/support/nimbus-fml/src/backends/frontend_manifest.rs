/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{BTreeMap, HashMap};

use crate::frontend::{
    EnumBody, EnumVariantBody, FeatureBody, FieldBody, ManifestFrontEnd, ObjectBody, Types,
};
use crate::intermediate_representation::{
    EnumDef, FeatureDef, FeatureManifest, ObjectDef, PropDef, VariantDef,
};

impl From<FeatureManifest> for ManifestFrontEnd {
    fn from(value: FeatureManifest) -> Self {
        let features = merge(&value, |fm| fm.feature_defs.clone(), |f| &f.name);
        let objects = merge(&value, |fm| fm.obj_defs.clone(), |o| &o.name);
        let enums = merge(&value, |fm| fm.enum_defs.clone(), |e| &e.name);

        let about = value.about.description_only();

        ManifestFrontEnd {
            about: Some(about),
            version: "1.0.0".to_string(),
            channels: vec![value.channel],
            includes: Default::default(),
            imports: Default::default(),
            features,
            legacy_types: None,
            types: Types { enums, objects },
        }
    }
}

fn merge<ListGetter, NameGetter, S, T>(
    root: &FeatureManifest,
    list_getter: ListGetter,
    name_getter: NameGetter,
) -> BTreeMap<String, T>
where
    T: From<S>,
    ListGetter: Fn(&FeatureManifest) -> Vec<S>,
    NameGetter: Fn(&S) -> &str,
{
    let mut dest: BTreeMap<String, T> = BTreeMap::new();

    for s in list_getter(root) {
        dest.insert(name_getter(&s).to_string(), s.into());
    }

    for fm in root.all_imports.values() {
        for s in list_getter(fm) {
            dest.insert(name_getter(&s).to_string(), s.into());
        }
    }

    dest
}

impl From<FeatureDef> for FeatureBody {
    fn from(value: FeatureDef) -> Self {
        let mut variables = BTreeMap::new();
        for f in value.props {
            variables.insert(f.name(), f.into());
        }

        Self {
            description: value.doc,
            variables,
            default: None,
        }
    }
}

impl From<ObjectDef> for ObjectBody {
    fn from(value: ObjectDef) -> Self {
        let mut fields = BTreeMap::new();
        for f in value.props {
            fields.insert(f.name.clone(), f.into());
        }

        Self {
            description: value.doc,
            fields,
        }
    }
}

impl From<EnumDef> for EnumBody {
    fn from(value: EnumDef) -> Self {
        let mut variants = HashMap::new();
        for v in value.variants {
            variants.insert(v.name.clone(), v.into());
        }
        Self {
            description: value.doc,
            variants,
        }
    }
}

impl From<VariantDef> for EnumVariantBody {
    fn from(value: VariantDef) -> Self {
        Self {
            description: value.doc,
        }
    }
}

impl From<PropDef> for FieldBody {
    fn from(value: PropDef) -> Self {
        Self {
            description: value.doc,
            variable_type: value.typ.to_string(),
            default: Some(value.default),
        }
    }
}
