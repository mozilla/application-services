/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::merge_kinds::*;
use crate::error::*;
use crate::ms_time::EARLIEST_SANE_TIME;
use crate::{JsonObject, JsonValue, Sym, SymMap};
use std::collections::HashSet;
use std::sync::Arc;
use url::Url;

/// The set of features understood by this client.
pub const REMERGE_FEATURES_UNDERSTOOD: &[&str] = &["record_set"];

/// The unserialized representation of the schema, parsed from a `RawSchema` (in
/// json.rs). If you change this, you may have to change that as well.
pub struct RecordSchema {
    pub name: Sym,
    pub version: semver::Version,
    pub required_version: semver::VersionReq,

    pub remerge_features_used: Vec<Sym>,

    pub legacy: bool,
    pub fields: SymMap<Field>,

    // pub field_map: HashMap<String, FieldIndex>,
    pub dedupe_on: Vec<Sym>,

    pub composite_roots: Vec<Sym>,
    pub composite_fields: Vec<Sym>,

    // If we have a semantic for an UpdatedAt Timestamp, it's this.
    pub field_updated_at: Option<Sym>,

    // If we have an own_guid field, it's this.
    pub field_own_guid: Sym,

    pub raw: super::json::RawSchema,
    pub source: Arc<str>,
}

impl std::fmt::Debug for RecordSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordSchema")
            .field("name", &self.name)
            .field("version", &self.version)
            .finish()
    }
}

impl RecordSchema {
    pub fn from_local(s: impl Into<Arc<str>>) -> Result<Arc<Self>, crate::SchemaError> {
        crate::schema::parse_from_string(s, false).map(Arc::new)
    }
    pub fn from_remote(s: impl Into<Arc<str>>) -> Result<Arc<Self>, crate::SchemaError> {
        crate::schema::parse_from_string(s, true).map(Arc::new)
    }
}

impl PartialEq for RecordSchema {
    fn eq(&self, o: &Self) -> bool {
        self.name == o.name
            && self.version == o.version
            && (self.source == o.source || self.raw == o.raw)
    }
}

impl RecordSchema {
    pub fn own_guid(&self) -> &Field {
        &self.fields[&self.field_own_guid]
    }
    pub fn field<'a, S: ?Sized + AsRef<str>>(&'a self, name: &S) -> Option<&'a Field> {
        self.fields.get(name)
    }
}

impl std::ops::Index<&Sym> for RecordSchema {
    type Output = Field;
    fn index(&self, idx: &Sym) -> &Field {
        &self.fields[idx]
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum CompositeInfo {
    Member { root: Sym },
    Root { children: Vec<Sym> },
}

/// A single field in a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: Sym,
    // Note: frequently equal to name.
    pub local_name: Sym,

    pub required: bool,
    pub deprecated: bool,

    pub change_preference: Option<ChangePreference>,
    pub composite: Option<CompositeInfo>,

    /// The type-specific information about a field.
    pub ty: FieldType,
}

impl Field {
    pub(crate) fn validate_guid(name: &Sym, v: &JsonValue) -> Result<crate::Guid, InvalidRecord> {
        if let JsonValue::String(s) = v {
            if s.len() < 8 || !crate::Guid::from(s.as_str()).is_valid_for_sync_server() {
                throw!(InvalidRecord::InvalidGuid(name.clone()))
            } else {
                Ok(crate::Guid::from(s.as_str()))
            }
        } else {
            throw!(InvalidRecord::WrongFieldType(
                name.clone(),
                FieldKind::OwnGuid
            ));
        }
    }

    pub fn validate(&self, v: JsonValue) -> Result<JsonValue> {
        // TODO(issue 2232): most errors should be more specific.
        use InvalidRecord::*;
        if !self.required && v.is_null() {
            return Ok(v);
        }
        match &self.ty {
            FieldType::Untyped { .. } => Ok(v),
            FieldType::OwnGuid { .. } => Ok(Self::validate_guid(&self.name, &v).map(|_| v)?),
            FieldType::Text { .. } => {
                if v.is_string() {
                    Ok(v)
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }
            FieldType::Boolean { .. } => {
                if let JsonValue::Bool(b) = v {
                    Ok(JsonValue::Bool(b))
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }

            FieldType::UntypedMap { .. } => {
                if v.is_object() {
                    Ok(v)
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }

            FieldType::RecordSet { id_key, .. } => self.validate_record_set(id_key.as_str(), v),
            FieldType::Url { is_origin, .. } => {
                if let JsonValue::String(s) = v {
                    if let Ok(url) = Url::parse(&s) {
                        if *is_origin {
                            let o = url.origin();
                            if !o.is_tuple() {
                                throw!(OriginWasOpaque(self.name.clone()));
                            }
                            if url.username() != ""
                                || url.password().is_some()
                                || url.path() != "/"
                                || url.query().is_some()
                                || url.fragment().is_some()
                            {
                                throw!(UrlWasNotOrigin(self.name.clone()));
                            }
                            // Truncate value to just origin
                            Ok(o.ascii_serialization().into())
                        } else {
                            Ok(url.to_string().into())
                        }
                    } else {
                        throw!(NotUrl(self.name.clone()));
                    }
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }
            FieldType::Real {
                min,
                max,
                if_out_of_bounds,
                ..
            } => {
                if let JsonValue::Number(n) = v {
                    let v = n
                        .as_f64()
                        .ok_or_else(|| WrongFieldType(self.name.clone(), self.ty.kind()))?;
                    self.validate_num(v, *min, *max, *if_out_of_bounds)
                        .map(JsonValue::from)
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }

            FieldType::Integer {
                min,
                max,
                if_out_of_bounds,
                ..
            } => {
                if let JsonValue::Number(n) = v {
                    let v = n
                        .as_i64()
                        .ok_or_else(|| WrongFieldType(self.name.clone(), self.ty.kind()))?;
                    self.validate_num(v, *min, *max, *if_out_of_bounds)
                        .map(JsonValue::from)
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }

            FieldType::Timestamp { .. } => {
                // We don't really have enough info to validate `semantic` here
                // (See also comments in `native_to_local` in `storage::info`),
                // so we don't check it.
                if let JsonValue::Number(n) = v {
                    let v = n
                        .as_i64()
                        .ok_or_else(|| WrongFieldType(self.name.clone(), self.ty.kind()))?;
                    if v <= EARLIEST_SANE_TIME {
                        throw!(OutOfBounds(self.name.clone()));
                    }
                    Ok(v.into())
                } else {
                    throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
                }
            }
        }
    }

    fn validate_record_set(&self, id_key: &str, v: JsonValue) -> Result<JsonValue> {
        use InvalidRecord::*;
        if let JsonValue::Array(a) = v {
            let mut seen: HashSet<&str> = HashSet::with_capacity(a.len());
            for item in &a {
                if let JsonValue::Object(o) = item {
                    if let Some(JsonValue::String(k)) = o.get(id_key) {
                        if seen.contains(k.as_str()) {
                            log::trace!(
                                "Record set entry {:?} has id_key {:?} more than once",
                                item,
                                id_key
                            );
                            throw!(InvalidRecordSet(self.name.clone()));
                        }
                        seen.insert(k.as_str());
                    } else {
                        log::trace!(
                            "Invalid id for id_key {:?} in record_set entry {:?}",
                            id_key,
                            item,
                        );
                        throw!(InvalidRecordSet(self.name.clone()));
                    }
                } else {
                    log::trace!("Record set entry {:?} is not an object", item);
                    throw!(InvalidRecordSet(self.name.clone()));
                }
            }
            Ok(JsonValue::Array(a))
        } else {
            throw!(WrongFieldType(self.name.clone(), self.ty.kind()));
        }
    }

    fn validate_num<N: PartialOrd + Copy>(
        &self,
        val: N,
        min: Option<N>,
        max: Option<N>,
        if_oob: IfOutOfBounds,
    ) -> Result<N> {
        let mut vc = val;
        if let Some(min) = min {
            if vc < min {
                vc = min;
            }
        }
        if let Some(max) = max {
            if vc > max {
                vc = max;
            }
        }
        if vc != val {
            match if_oob {
                IfOutOfBounds::Discard => {
                    throw!(crate::error::InvalidRecord::OutOfBounds(self.name.clone()))
                }
                IfOutOfBounds::Clamp => Ok(vc),
            }
        } else {
            Ok(val)
        }
    }

    pub fn timestamp_semantic(&self) -> Option<TimestampSemantic> {
        match &self.ty {
            FieldType::Timestamp { semantic, .. } => *semantic,
            _ => None,
        }
    }

    pub fn is_kind(&self, k: FieldKind) -> bool {
        self.ty.is_kind(k)
    }
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

    Real {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangePreference {
    Missing,
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
    Real,
    Integer,
    Timestamp,
    Boolean,
    OwnGuid,
    UntypedMap,
    RecordSet,
}

impl std::fmt::Display for FieldKind {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FieldKind::Untyped => "untyped",
            FieldKind::Text => "text",
            FieldKind::Url => "url",
            FieldKind::Real => "real",
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
    pub fn kind(&self) -> FieldKind {
        match self {
            FieldType::Untyped { .. } => FieldKind::Untyped,
            FieldType::Text { .. } => FieldKind::Text,
            FieldType::Url { .. } => FieldKind::Url,

            FieldType::Real { .. } => FieldKind::Real,
            FieldType::Integer { .. } => FieldKind::Integer,
            FieldType::Timestamp { .. } => FieldKind::Timestamp,

            FieldType::Boolean { .. } => FieldKind::Boolean,
            FieldType::OwnGuid { .. } => FieldKind::OwnGuid,
            FieldType::UntypedMap { .. } => FieldKind::UntypedMap,
            FieldType::RecordSet { .. } => FieldKind::RecordSet,
        }
    }

    pub fn is_kind(&self, k: FieldKind) -> bool {
        self.kind() == k
    }

    pub fn uses_untyped_merge(&self, um: UntypedMerge) -> bool {
        match self {
            // These branches must be separate since many of the `merge`s
            // have diff. types, but they all impl PartialEq<UntypedMerge>.
            FieldType::Untyped { merge, .. } => &um == merge,
            FieldType::Text { merge, .. } | FieldType::Url { merge, .. } => &um == merge,
            FieldType::Real { merge, .. } | FieldType::Integer { merge, .. } => &um == merge,
            FieldType::Timestamp { merge, .. } => &um == merge,
            FieldType::Boolean { merge, .. } => &um == merge,

            // List these out so new additions need to update this.
            FieldType::OwnGuid { .. }
            | FieldType::UntypedMap { .. }
            | FieldType::RecordSet { .. } => false,
        }
    }
    pub fn get_default(&self) -> Option<JsonValue> {
        match self {
            FieldType::Untyped { default, .. } => default.clone(),
            FieldType::Text { default, .. } => default.as_ref().map(|s| s.as_str().into()),
            FieldType::Url { default, .. } => default.as_ref().map(|s| s.to_string().into()),
            FieldType::Real { default, .. } => default.map(|s| s.into()),
            FieldType::Integer { default, .. } => default.map(|s| s.into()),
            FieldType::Timestamp {
                default: Some(TimestampDefault::Now),
                ..
            } => Some(crate::ms_time::MsTime::now().into()),
            FieldType::Timestamp {
                default: Some(TimestampDefault::Value(v)),
                ..
            } => Some((*v).into()),
            FieldType::Timestamp { default: None, .. } => None,
            FieldType::Boolean { default, .. } => default.map(|s| s.into()),
            FieldType::OwnGuid { .. } => None,
            FieldType::UntypedMap { default, .. } => {
                default.as_ref().map(|s| JsonValue::Object(s.clone()))
            }
            FieldType::RecordSet { default, .. } => default.as_ref().map(|s| {
                JsonValue::Array(s.iter().map(|v| JsonValue::Object(v.clone())).collect())
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum IfOutOfBounds {
    Clamp,
    Discard,
}
