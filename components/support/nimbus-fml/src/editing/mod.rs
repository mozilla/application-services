/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error_path;

pub(crate) use error_path::ErrorPath;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    error::FMLError,
    intermediate_representation::{EnumDef, FeatureDef, ObjectDef},
};

pub(crate) struct FeatureValidationError {
    pub(crate) path: ErrorPath,
    pub(crate) message: String,
}

impl From<FeatureValidationError> for FMLError {
    fn from(value: FeatureValidationError) -> Self {
        Self::ValidationError(value.path.path, value.message)
    }
}

#[allow(dead_code)]
pub(crate) struct ErrorConverter<'a> {
    enum_defs: &'a BTreeMap<String, EnumDef>,
    object_defs: &'a BTreeMap<String, ObjectDef>,
}

impl<'a> ErrorConverter<'a> {
    pub(crate) fn new(
        enum_defs: &'a BTreeMap<String, EnumDef>,
        object_defs: &'a BTreeMap<String, ObjectDef>,
    ) -> Self {
        Self {
            enum_defs,
            object_defs,
        }
    }

    pub(crate) fn convert_feature_error(
        &self,
        _feature_def: &FeatureDef,
        _value: &Value,
        error: FeatureValidationError,
    ) -> FMLError {
        error.into()
    }

    pub(crate) fn convert_object_error(&self, error: FeatureValidationError) -> FMLError {
        error.into()
    }
}
