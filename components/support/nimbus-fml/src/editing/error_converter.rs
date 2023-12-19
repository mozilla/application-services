/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "client-lib")]
use super::FmlEditorError;
use super::{values_finder::ValuesFinder, ErrorKind, FeatureValidationError};
use crate::{
    error::FMLError,
    intermediate_representation::{EnumDef, FeatureDef, ObjectDef},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[allow(dead_code)]
pub(crate) struct ErrorConverter<'a> {
    pub(crate) enum_defs: &'a BTreeMap<String, EnumDef>,
    pub(crate) object_defs: &'a BTreeMap<String, ObjectDef>,
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
        feature_def: &FeatureDef,
        feature_value: &Value,
        error: FeatureValidationError,
    ) -> FMLError {
        let values = ValuesFinder::new(self.enum_defs, feature_def, feature_value);
        let long_message = self.long_message(&values, &error);
        FMLError::ValidationError(error.path.path, long_message)
    }

    #[allow(dead_code)]
    #[cfg(feature = "client-lib")]
    pub(crate) fn convert_into_editor_errors(
        &self,
        feature_def: &FeatureDef,
        feature_value: &Value,
        src: &str,
        errors: &Vec<FeatureValidationError>,
    ) -> Vec<FmlEditorError> {
        let mut editor_errors: Vec<_> = Default::default();
        let values = ValuesFinder::new(self.enum_defs, feature_def, feature_value);
        for e in errors {
            let message = self.long_message(&values, e);
            let highlight = e.path.last_token().map(str::to_string);
            let (line, col) = e.path.line_col(src);
            let error = FmlEditorError {
                message,
                line: line as u32,
                col: col as u32,
                highlight,
            };
            editor_errors.push(error);
        }
        editor_errors
    }

    pub(crate) fn convert_object_error(&self, error: FeatureValidationError) -> FMLError {
        FMLError::ValidationError(error.path.path.to_owned(), self.message(&error))
    }
}

impl ErrorConverter<'_> {
    fn long_message(&self, values: &ValuesFinder, error: &FeatureValidationError) -> String {
        let message = self.message(error);
        let mut suggestions = self.suggested_replacements(error, values);
        let dym = did_you_mean(&mut suggestions);
        format!("{message}{dym}")
    }

    fn message(&self, error: &FeatureValidationError) -> String {
        let token = error.path.last_token().unwrap_or("unknown");
        error.kind.message(token)
    }

    fn suggested_replacements(
        &self,
        error: &FeatureValidationError,
        values: &ValuesFinder,
    ) -> BTreeSet<String> {
        let complete = match &error.kind {
            ErrorKind::InvalidKey { key_type: t, .. }
            | ErrorKind::InvalidValue { value_type: t, .. }
            | ErrorKind::TypeMismatch { value_type: t } => values.all(t),
            ErrorKind::InvalidPropKey { valid, .. } => valid.to_owned(),
            ErrorKind::InvalidNestedValue { .. } => Default::default(),
        };

        // We don't want to suggest any tokens that the user has already used correctly, so
        // we can filter out the ones in use.
        match &error.kind {
            ErrorKind::InvalidKey { in_use, .. } | ErrorKind::InvalidPropKey { in_use, .. }
                // This last check is an optimization:
                // if none of the in_use are valid,
                // then we can skip cloning.
                if !complete.is_disjoint(in_use) =>
            {
                complete.difference(in_use).cloned().collect()
            }
            _ => complete,
        }
    }
}

fn did_you_mean(words: &mut BTreeSet<String>) -> String {
    let mut words = words.iter();
    match words.len() {
        0 => String::from(""),
        1 => format!("; did you mean \"{}\"?", words.next().unwrap()),
        2 => format!(
            "; did you mean \"{}\" or \"{}\"?",
            words.next().unwrap(),
            words.next().unwrap(),
        ),
        _ => {
            let last = words.next_back().unwrap();
            format!(
                "; did you mean one of \"{}\" or \"{last}\"?",
                itertools::join(words, "\", \"")
            )
        }
    }
}
