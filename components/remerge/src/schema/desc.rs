/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::merge_kinds::*;
use index_vec::IndexVec;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use url::Url;

/// The set of features understood by this client.
pub const REMERGE_FEATURES_UNDERSTOOD: &[&str] = &["record_set", "untyped_map"];

pub type JsonObject = serde_json::Map<String, JsonValue>;

index_vec::define_index_type! {
    /// Newtype wrapper around usize, referring into the `fields` vec in a
    /// RecordSchema
    pub struct FieldIndex = usize;
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordSchema {
    pub version: semver::Version,
    pub required_version: semver::VersionReq,

    pub remerge_features_used: Vec<String>,

    pub legacy: bool,
    pub fields: IndexVec<FieldIndex, Field>,
    pub field_map: HashMap<String, FieldIndex>,

    pub dedupe_on: Vec<FieldIndex>,

    pub composite_roots: Vec<FieldIndex>,
    pub composite_fields: Vec<FieldIndex>,

    // If we have a semantic for an UpdatedAt Timestamp, it's this.
    pub field_updated_at: Option<FieldIndex>,

    // If we have an own_guid field, it's this.
    pub field_own_guid: Option<FieldIndex>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CompositeInfo {
    Member { root: FieldIndex },
    Root { children: Vec<FieldIndex> },
}

/// A single field in a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub local_name: String,

    pub required: bool,
    pub deprecated: bool,

    pub change_preference: Option<ChangePreference>,
    pub composite: Option<CompositeInfo>,

    /// The type-specific information about a field.
    pub ty: FieldType,
    pub own_idx: FieldIndex,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldType {
    Untyped {
        merge: UntypedMerge,
        default: Option<JsonValue>,
    },

    Text {
        merge: TextMerge,
        default: Option<String>,
    },

    Url {
        merge: TextMerge,
        is_origin: bool,
        default: Option<Url>,
    },

    Number {
        merge: NumberMerge,
        min: Option<f64>,
        max: Option<f64>,
        if_out_of_bounds: IfOutOfBounds,
        default: Option<f64>,
    },

    Integer {
        merge: NumberMerge,
        min: Option<i64>,
        max: Option<i64>,
        if_out_of_bounds: IfOutOfBounds,
        default: Option<i64>,
    },

    Timestamp {
        merge: TimestampMerge,
        semantic: Option<TimestampSemantic>,
        default: Option<TimestampDefault>,
    },

    Boolean {
        merge: BooleanMerge,
        default: Option<bool>,
    },

    OwnGuid {
        auto: bool,
    },

    UntypedMap {
        prefer_deletions: bool,
        default: Option<JsonObject>,
    },

    RecordSet {
        id_key: String,
        prefer_deletions: bool,
        default: Option<Vec<JsonObject>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimestampSemantic {
    CreatedAt,
    UpdatedAt,
}

impl std::fmt::Display for TimestampSemantic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimestampSemantic::CreatedAt => f.write_str("created_at"),
            TimestampSemantic::UpdatedAt => f.write_str("updated_at"),
        }
    }
}

impl TimestampSemantic {
    pub fn required_merge(self) -> TimestampMerge {
        match self {
            TimestampSemantic::CreatedAt => TimestampMerge::TakeMin,
            TimestampSemantic::UpdatedAt => TimestampMerge::TakeMax,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangePreference {
    #[serde(rename = "missing")]
    Missing,

    #[serde(rename = "present")]
    Present,
}

// We handle serialization specially.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimestampDefault {
    Value(i64),
    Now,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldKind {
    Untyped,
    Text,
    Url,
    Number,
    Integer,
    Timestamp,
    Boolean,
    OwnGuid,
    UntypedMap,
    RecordSet,
}

impl std::fmt::Display for FieldKind {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            FieldKind::Untyped => "untyped",
            FieldKind::Text => "text",
            FieldKind::Url => "url",
            FieldKind::Number => "number",
            FieldKind::Integer => "integer",
            FieldKind::Timestamp => "timestamp",
            FieldKind::Boolean => "boolean",
            FieldKind::OwnGuid => "own_guid",
            FieldKind::UntypedMap => "untyped_map",
            FieldKind::RecordSet => "record_set",
        })
    }
}

impl FieldType {
    pub fn is_kind(&self, k: FieldKind) -> bool {
        match self {
            FieldType::Untyped { .. } => k == FieldKind::Untyped,
            FieldType::Text { .. } => k == FieldKind::Text,
            FieldType::Url { .. } => k == FieldKind::Url,

            FieldType::Number { .. } => k == FieldKind::Number,
            FieldType::Integer { .. } => k == FieldKind::Integer,
            FieldType::Timestamp { .. } => k == FieldKind::Timestamp,

            FieldType::Boolean { .. } => k == FieldKind::Boolean,
            FieldType::OwnGuid { .. } => k == FieldKind::OwnGuid,
            FieldType::UntypedMap { .. } => k == FieldKind::UntypedMap,
            FieldType::RecordSet { .. } => k == FieldKind::RecordSet,
        }
    }

    pub fn uses_untyped_merge(&self, um: UntypedMerge) -> bool {
        match self {
            // These branches must be separate since many of the `merge`s
            // have diff. types, but they all impl PartialEq<UntypedMerge>.
            FieldType::Untyped { merge, .. } => &um == merge,
            FieldType::Text { merge, .. } | FieldType::Url { merge, .. } => &um == merge,
            FieldType::Number { merge, .. } | FieldType::Integer { merge, .. } => &um == merge,
            FieldType::Timestamp { merge, .. } => &um == merge,
            FieldType::Boolean { merge, .. } => &um == merge,

            // List these out so new additions need to update this.
            FieldType::OwnGuid { .. }
            | FieldType::UntypedMap { .. }
            | FieldType::RecordSet { .. } => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub enum IfOutOfBounds {
    #[serde(rename = "clamp")]
    Clamp,
    #[serde(rename = "discard")]
    Discard,
}
