/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module is concerned primarially with schema parsing (from RawSchema,
//! e.g. the schema represented as JSON), and validation. It's a little bit
//! hairy, and for the definitive documentation, you should refer to the
//! `docs/design/remerge/schema-format.md` docs.

// Clippy seems to be upset about serde's output:
// https://github.com/rust-lang/rust-clippy/issues/4326
#![allow(clippy::type_repetition_in_bounds)]

use super::desc::*;
use super::error::*;
use super::merge_kinds::*;
use crate::util::is_default;
use crate::{JsonObject, JsonValue};
use crate::{Sym, SymMap, SymObject};
use matches::matches;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Arc;
use url::Url;

pub const FORMAT_VERSION: i64 = 1;

pub fn parse_from_string(json: impl Into<Arc<str>>, is_remote: bool) -> SchemaResult<RecordSchema> {
    parse_from_string_impl(json.into(), is_remote)
}

fn parse_from_string_impl(json: Arc<str>, is_remote: bool) -> SchemaResult<RecordSchema> {
    let raw = match serde_json::from_str::<RawSchema>(&json) {
        Ok(schema) => schema,
        Err(e) => {
            // If it's local then this is just a format error.
            if !is_remote {
                // For some reason throw! and ensure! both complain about moving
                // `e` here, but this works...
                throw!(SchemaError::FormatError(e));
            }
            if let Ok(dumb) = serde_json::from_str::<DumbSchema>(&json) {
                // TODO: Spec says we should treat these as `untyped` if for
                // remote records if we aren't locked out!
                for field in &dumb.fields {
                    if let Some(ty) = field.get("type").and_then(|f| f.as_str()) {
                        if !KNOWN_FIELD_TYPE_TAGS.contains(&ty) {
                            throw!(SchemaError::UnknownFieldType(ty.to_owned()));
                        }
                    }
                }
            }
            // Can't use map_err without moving `e` (original error), which,
            // unless we find something better, will probably be the most
            // accurate
            let as_json: JsonObject = match serde_json::from_str(&json) {
                Ok(o) => o,
                Err(_) => throw!(SchemaError::FormatError(e)),
            };

            // If it's remote, then it failed, but it could have failed because
            // it's from a future version. Check that.
            let version = match as_json.get("format_version") {
                Some(JsonValue::Number(n)) if n.is_i64() => n.as_i64().unwrap(),
                _ => {
                    // Ditto with moving `e` (which we want to use because it can give
                    // better error messages).
                    throw!(SchemaError::FormatError(e));
                }
            };
            if version != FORMAT_VERSION {
                throw!(SchemaError::WrongFormatVersion(version))
            } else {
                throw!(SchemaError::FormatError(e))
            };
        }
    };
    let parser = SchemaParser::new(&raw, is_remote)?;
    let mut result = parser.parse(json)?;
    result.raw = raw;
    Ok(result)
}

/// Helper trait to make marking results / errors with which field were were
/// parsing more convenient.
trait FieldErrorHelper {
    type Out;
    fn named(self, name: &str) -> Self::Out;
}

impl FieldErrorHelper for FieldError {
    type Out = Box<SchemaError>;
    fn named(self, name: &str) -> Self::Out {
        Box::new(SchemaError::FieldError(name.into(), self))
    }
}

impl<T> FieldErrorHelper for Result<T, FieldError> {
    type Out = SchemaResult<T>;
    fn named(self, name: &str) -> Self::Out {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(e.named(name)),
        }
    }
}

/// The serialized representation of the schema.
///
/// Note that if you change this, you will likely have to change the data in
/// `schema/desc.rs`.
///
/// Important: Note that changes to this are in general not allowed to fail to
/// parse older versions of this format.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSchemaInfo<FT> {
    /// The name of this collection
    pub name: Sym,
    /// The version of the schema
    pub version: String,

    /// The required version of the schema
    pub required_version: Option<String>,

    #[serde(default)]
    pub remerge_features_used: Vec<Sym>,

    #[serde(default)]
    pub legacy: bool,

    pub fields: Vec<FT>,

    #[serde(default)]
    pub dedupe_on: Vec<Sym>,

    #[serde(flatten)]
    pub unknown: crate::JsonObject,
}

/// Unvalidated but mostly-understood schema
pub type RawSchema = RawSchemaInfo<RawFieldType>;

impl RawSchema {
    /// Returns true if there were no unknown enums, leftover fields in objects,
    /// etc.
    pub fn is_understood(&self) -> bool {
        self.unknown.is_empty() && self.fields.iter().all(|v| v.is_understood())
    }
}

/// Schema containing a field type we don't know about.
pub type DumbSchema = RawSchemaInfo<SymObject>;

// Can't derive this :(...
impl<FT> Default for RawSchemaInfo<FT> {
    fn default() -> Self {
        Self {
            name: Sym::default(),
            version: "".to_owned(),
            required_version: None,
            remerge_features_used: vec![],
            legacy: false,
            fields: vec![],
            dedupe_on: vec![],
            unknown: JsonObject::new(),
        }
    }
}

// OptDefaultType not just being the type and made into an Option here is for serde's benefit.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct RawFieldCommon<OptDefaultType: PartialEq + Default> {
    pub name: Sym,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub local_name: Option<Sym>,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub required: bool,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub deprecated: bool,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub composite_root: Option<Sym>,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub merge: Option<ParsedMerge>,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub change_preference: Option<RawChangePreference>,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub default: OptDefaultType,

    #[serde(flatten)]
    pub unknown: SymObject,
}

const KNOWN_FIELD_TYPE_TAGS: &[&str] = &[
    "untyped",
    "text",
    "url",
    "boolean",
    "real",
    "integer",
    "timestamp",
    "own_guid",
    "untyped_map",
    "record_set",
];

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum RawFieldType {
    #[serde(rename = "untyped")]
    Untyped {
        #[serde(flatten)]
        common: RawFieldCommon<Option<JsonValue>>,
    },

    #[serde(rename = "text")]
    Text {
        #[serde(flatten)]
        common: RawFieldCommon<Option<String>>,
    },

    #[serde(rename = "url")]
    Url {
        #[serde(flatten)]
        common: RawFieldCommon<Option<String>>, // XXX: Option<Url>...
        #[serde(default)]
        is_origin: bool,
    },

    #[serde(rename = "boolean")]
    Boolean {
        #[serde(flatten)]
        common: RawFieldCommon<Option<bool>>,
    },

    #[serde(rename = "real")]
    Real {
        #[serde(flatten)]
        common: RawFieldCommon<Option<f64>>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        min: Option<f64>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        max: Option<f64>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        if_out_of_bounds: Option<RawIfOutOfBounds>,
    },

    #[serde(rename = "integer")]
    Integer {
        #[serde(flatten)]
        common: RawFieldCommon<Option<i64>>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        min: Option<i64>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        max: Option<i64>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        if_out_of_bounds: Option<RawIfOutOfBounds>,
    },

    #[serde(rename = "timestamp")]
    Timestamp {
        #[serde(flatten)]
        common: RawFieldCommon<Option<RawTimeDefault>>,

        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        semantic: Option<RawTimestampSemantic>,
    },

    #[serde(rename = "own_guid")]
    OwnGuid {
        #[serde(flatten)]
        // TODO: check that serde does what I want with the `()` field.
        common: RawFieldCommon<()>,
        #[serde(default)]
        #[serde(skip_serializing_if = "is_default")]
        auto: Option<bool>,
    },

    #[serde(rename = "untyped_map")]
    UntypedMap {
        #[serde(flatten)]
        common: RawFieldCommon<Option<JsonObject>>,

        #[serde(default)]
        prefer_deletions: bool,
    },

    #[serde(rename = "record_set")]
    RecordSet {
        #[serde(flatten)]
        common: RawFieldCommon<Option<Vec<JsonObject>>>,

        // Note: required!
        id_key: String,

        #[serde(default)]
        prefer_deletions: bool,
    },
    // TODO:
    // #[serde(other)] Unknown,
    // but see if we can save the JSON by using a custom deserializer...
}

impl RawFieldType {
    pub fn is_understood(&self) -> bool {
        if !self.unknown().is_empty() {
            return false;
        }
        if let Some(RawChangePreference::Unknown(_)) = self.change_preference() {
            return false;
        }
        if let Some(ParsedMerge::Unknown(_)) = self.merge() {
            return false;
        }
        match self {
            RawFieldType::Integer {
                if_out_of_bounds: Some(RawIfOutOfBounds::Unknown(_)),
                ..
            } => false,
            RawFieldType::Real {
                if_out_of_bounds: Some(RawIfOutOfBounds::Unknown(_)),
                ..
            } => false,
            RawFieldType::Timestamp { semantic, common } => {
                if let Some(RawTimestampSemantic::Unknown(_)) = semantic {
                    false
                } else if let Some(RawTimeDefault::Special(RawSpecialTime::Unknown(_))) =
                    &common.default
                {
                    false
                } else {
                    true
                }
            }
            _ => true,
        }
    }
}

macro_rules! common_getter {
    ($name:ident, $T:ty) => {
        pub fn $name(&self) -> $T {
            match self {
                RawFieldType::Untyped { common, .. } => &common.$name,
                RawFieldType::Text { common, .. } => &common.$name,
                RawFieldType::Url { common, .. } => &common.$name,
                RawFieldType::Boolean { common, .. } => &common.$name,
                RawFieldType::Real { common, .. } => &common.$name,
                RawFieldType::Integer { common, .. } => &common.$name,
                RawFieldType::Timestamp { common, .. } => &common.$name,
                RawFieldType::OwnGuid { common, .. } => &common.$name,
                RawFieldType::RecordSet { common, .. } => &common.$name,
                RawFieldType::UntypedMap { common, .. } => &common.$name,
            }
        }
    };
}

impl RawFieldType {
    common_getter!(unknown, &SymObject);
    common_getter!(name, &Sym);
    common_getter!(local_name, &Option<Sym>);
    common_getter!(required, &bool);
    common_getter!(deprecated, &bool);
    common_getter!(composite_root, &Option<Sym>);
    common_getter!(merge, &Option<ParsedMerge>);
    common_getter!(change_preference, &Option<RawChangePreference>);

    pub fn kind(&self) -> FieldKind {
        match self {
            RawFieldType::Untyped { .. } => FieldKind::Untyped,
            RawFieldType::Text { .. } => FieldKind::Text,
            RawFieldType::Url { .. } => FieldKind::Url,
            RawFieldType::Real { .. } => FieldKind::Real,
            RawFieldType::Integer { .. } => FieldKind::Integer,
            RawFieldType::Timestamp { .. } => FieldKind::Timestamp,
            RawFieldType::Boolean { .. } => FieldKind::Boolean,
            RawFieldType::OwnGuid { .. } => FieldKind::OwnGuid,
            RawFieldType::UntypedMap { .. } => FieldKind::UntypedMap,
            RawFieldType::RecordSet { .. } => FieldKind::RecordSet,
        }
    }

    pub fn get_merge(&self) -> Result<Option<ParsedMerge>, FieldError> {
        self.merge()
            .clone()
            .map(Ok)
            .or_else(|| match self {
                RawFieldType::Timestamp {
                    semantic: Some(RawTimestampSemantic::CreatedAt),
                    ..
                } => Some(Ok(ParsedMerge::TakeMin)),
                RawFieldType::Timestamp {
                    semantic: Some(RawTimestampSemantic::UpdatedAt),
                    ..
                } => Some(Ok(ParsedMerge::TakeMax)),
                RawFieldType::Timestamp {
                    semantic: Some(RawTimestampSemantic::Unknown(v)),
                    ..
                } => Some(Err(FieldError::UnknownVariant(format!(
                    "unknown timestamp semantic: {}",
                    v
                )))),
                _ => None,
            })
            .transpose()
    }

    pub fn has_default(&self) -> bool {
        match self {
            RawFieldType::Untyped { common, .. } => common.default.is_some(),
            RawFieldType::Text { common, .. } => common.default.is_some(),
            RawFieldType::Url { common, .. } => common.default.is_some(),
            RawFieldType::Boolean { common, .. } => common.default.is_some(),
            RawFieldType::Real { common, .. } => common.default.is_some(),
            RawFieldType::Integer { common, .. } => common.default.is_some(),
            RawFieldType::Timestamp { common, .. } => common.default.is_some(),
            RawFieldType::OwnGuid { .. } => false,
            RawFieldType::RecordSet { common, .. } => common.default.is_some(),
            RawFieldType::UntypedMap { common, .. } => common.default.is_some(),
        }
    }
}

/// This (and RawSpecialTime) are basically the same as TimestampDefault, just done
/// in such a way to make serde deserialize things the way we want for us.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawTimeDefault {
    Num(i64),
    Special(RawSpecialTime),
}

define_enum_with_unknown! {
    #[derive(Clone, PartialEq)]
    pub enum RawSpecialTime {
        Now = "now",
        .. Unknown(crate::Sym),
    }
}

define_enum_with_unknown! {
    #[derive(Clone, PartialEq)]
    pub enum RawTimestampSemantic {
        CreatedAt = "created_at",
        UpdatedAt = "updated_at",
        .. Unknown(crate::Sym),
    }
    IMPL_FROM_RAW = TimestampSemantic;
}

define_enum_with_unknown! {
    #[derive(Clone, PartialEq)]
    pub enum RawChangePreference {
        Missing = "missing",
        Present = "present",
        .. Unknown(crate::Sym),
    }
    IMPL_FROM_RAW = ChangePreference;
    DERIVE_DISPLAY = true;
}

struct SchemaParser<'a> {
    input: &'a RawSchema,
    input_fields: SymMap<&'a RawFieldType>,

    parsed_fields: SymMap<Field>,
    dedupe_ons: BTreeSet<Sym>,
    possible_composite_roots: BTreeSet<Sym>,
    composite_members: BTreeSet<Sym>,
}

fn parse_version(v: &str, prop: SemverProp) -> SchemaResult<semver::Version> {
    semver::Version::parse(v).map_err(|err| {
        SchemaError::VersionParseFailed {
            got: v.into(),
            prop,
            err,
        }
        .into()
    })
}

fn parse_version_req(
    o: &Option<String>,
    prop: SemverProp,
) -> SchemaResult<Option<semver::VersionReq>> {
    // when transpose is stable this could be simpler...
    if let Some(v) = o {
        Ok(Some(semver::VersionReq::parse(v).map_err(|err| {
            SchemaError::VersionReqParseFailed {
                got: v.clone(),
                prop,
                err,
            }
        })?))
    } else {
        Ok(None)
    }
}

pub(crate) fn compatible_version_req(v: &semver::Version) -> semver::VersionReq {
    crate::util::compatible_version_req(v).unwrap_or_else(|e| {
        panic!(
            "Bug: Failed to parse our generated VersionReq from {:?}: {}",
            v, e
        );
    })
}

impl<'a> SchemaParser<'a> {
    pub fn new(repr: &'a RawSchema, _is_remote: bool) -> SchemaResult<Self> {
        let composite_roots = repr
            .fields
            .iter()
            .filter_map(|f| f.composite_root().clone())
            .collect::<BTreeSet<_>>();

        let composite_members = repr
            .fields
            .iter()
            .filter_map(|f| f.composite_root().as_ref().map(|_| f.name().into()))
            .chain(composite_roots.iter().cloned())
            .collect::<BTreeSet<_>>();

        let dedupe_on = repr.dedupe_on.iter().cloned().collect::<BTreeSet<_>>();
        ensure!(
            dedupe_on.len() == repr.dedupe_on.len(),
            SchemaError::RepeatedDedupeOn
        );

        Ok(Self {
            input: repr,
            input_fields: repr.fields.iter().map(|f| (f.name().clone(), f)).collect(),
            parsed_fields: SymMap::new(),
            possible_composite_roots: composite_roots,
            composite_members,
            dedupe_ons: dedupe_on,
        })
    }

    fn check_user_version(&self) -> SchemaResult<(semver::Version, semver::VersionReq)> {
        let cur_version = parse_version(&self.input.version, SemverProp::Version)?;
        let req_version =
            parse_version_req(&self.input.required_version, SemverProp::RequiredVersion)?
                .unwrap_or_else(|| compatible_version_req(&cur_version));

        ensure!(
            req_version.matches(&cur_version),
            SchemaError::LocalRequiredVersionNotCompatible(req_version, cur_version)
        );
        Ok((cur_version, req_version))
    }

    fn is_identity(&self, name: &str) -> bool {
        self.dedupe_ons.contains(name)
    }

    fn is_composite_root(&self, name: &str) -> bool {
        // Someone thinks it's a composite, at least.
        self.possible_composite_roots.contains(name)
    }

    pub fn parse(mut self, source: Arc<str>) -> SchemaResult<RecordSchema> {
        let (version, required_version) = self.check_user_version()?;

        let unknown_feat = self
            .input
            .remerge_features_used
            .iter()
            .find(|f| !REMERGE_FEATURES_UNDERSTOOD.contains(&f.as_str()));
        if let Some(f) = unknown_feat {
            throw!(SchemaError::MissingRemergeFeature(f.to_string()));
        }

        let mut own_guid: Option<Sym> = None;
        let mut updated_at: Option<Sym> = None;

        for field in &self.input.fields {
            let parsed = self.parse_field(field)?;
            // look for 'special' fields.
            match &parsed.ty {
                FieldType::OwnGuid { .. } => {
                    ensure!(own_guid.is_none(), SchemaError::MultipleOwnGuid);
                    own_guid = Some(parsed.name.clone());
                }
                FieldType::Timestamp {
                    semantic: Some(TimestampSemantic::UpdatedAt),
                    ..
                } => {
                    ensure!(updated_at.is_none(), SchemaError::MultipleUpdateAt);
                    updated_at = Some(parsed.name.clone());
                }
                _ => {}
            }

            self.parsed_fields.insert(parsed.name.clone(), parsed);
        }

        let is_legacy = self.input.legacy;

        self.check_dedupe_on()?;

        let (composite_roots, composite_fields) = self.composite_roots_fields();

        self.check_used_features(&self.input.remerge_features_used)?;
        let field_own_guid = own_guid.ok_or_else(|| SchemaError::MissingOwnGuid)?;

        Ok(RecordSchema {
            name: self.input.name.clone(),
            version,
            required_version,
            remerge_features_used: self.input.remerge_features_used.clone(),
            legacy: is_legacy,
            fields: self.parsed_fields,
            dedupe_on: self.dedupe_ons.clone(),
            composite_roots,
            composite_fields,
            // field_map: self.indices,
            field_updated_at: updated_at,
            field_own_guid,
            source,
            // Filled in at caller
            raw: RawSchema::default(),
        })
    }

    fn composite_roots_fields(&self) -> (BTreeSet<Sym>, BTreeSet<Sym>) {
        let composite_roots = self
            .parsed_fields
            .values()
            .filter(|f| matches!(f.composite, Some(CompositeInfo::Root { .. })))
            .map(|f| f.name.clone())
            .collect();

        let composite_fields = self
            .parsed_fields
            .values()
            .filter(|f| f.composite.is_some())
            .map(|f| f.name.clone())
            .collect();

        (composite_roots, composite_fields)
    }

    fn check_dedupe_on(&self) -> Result<(), SchemaError> {
        assert!(self.parsed_fields.len() == self.input_fields.len());

        for name in &self.input.dedupe_on {
            let field = self
                .parsed_fields
                .get(name)
                .ok_or_else(|| SchemaError::UnknownDedupeOnField(name.into()))?;

            if !self.composite_members.contains(name) {
                continue;
            }

            let root = match field.composite.as_ref().unwrap() {
                CompositeInfo::Member { root } => &self.parsed_fields[root],
                CompositeInfo::Root { .. } => &field,
            };
            let root_kids =
                if let CompositeInfo::Root { children } = &root.composite.as_ref().unwrap() {
                    children
                } else {
                    unreachable!("composite root isn't a root even though we just checked");
                };
            let all_id = std::iter::once(root.name.as_str())
                .chain(
                    root_kids
                        .iter()
                        .map(|k| self.parsed_fields[k].name.as_str()),
                )
                .all(|name| self.is_identity(name));
            ensure!(all_id, SchemaError::PartialCompositeDedupeOn);
        }
        Ok(())
    }

    fn parse_field(&self, field: &RawFieldType) -> SchemaResult<Field> {
        let field_name = field.name();
        let local_name = field.local_name().clone();
        self.check_field_name(field_name, &local_name)?;

        self.check_type_restrictions(field).named(field_name)?;

        let merge = field.get_merge().named(field_name)?;

        if field.composite_root().is_some() {
            self.check_composite_member_field(field, merge.clone())
                .named(field_name)?;
        }

        if self.is_composite_root(field_name) {
            self.check_composite_root_field(field, merge.clone())
                .named(field_name)?;
        }

        // using TakeNewest as the default is not really necessarially true.
        let merge = merge.unwrap_or(ParsedMerge::TakeNewest);

        let result_field_type: FieldType = self.get_field_type(merge, field)?;

        if result_field_type.uses_untyped_merge(UntypedMerge::Duplicate)
            && !self.input.dedupe_on.is_empty()
        {
            throw!(SchemaError::DedupeOnWithDuplicateField);
        }

        let deprecated = *field.deprecated();
        let required = *field.required();
        let change_preference = field
            .change_preference()
            .as_ref()
            .map(|v| {
                ChangePreference::from_raw(v).ok_or_else(|| {
                    FieldError::UnknownVariant(format!("Unknown change preference {}", v))
                })
            })
            .transpose()
            .named(field_name)?;

        if deprecated {
            ensure!(
                !self.is_identity(field.name()),
                SchemaError::DeprecatedFieldDedupeOn(field_name.into())
            );
        }
        ensure!(
            !(deprecated && required),
            FieldError::DeprecatedRequiredConflict.named(field_name)
        );

        let composite = self.get_composite_info(field);
        let name = field_name.clone();
        let f = Field {
            local_name: local_name.unwrap_or_else(|| name.clone()),
            name,
            deprecated,
            required,
            ty: result_field_type,
            change_preference,
            composite,
        };
        Ok(f)
    }

    // Note: Asserts if anything is wrong, caller is expected to check all that first.
    fn get_composite_info(&self, field: &RawFieldType) -> Option<CompositeInfo> {
        let field_name = field.name();
        if self.is_composite_root(field_name) {
            let children = self
                .input_fields
                .values()
                .filter(|f| f.composite_root().as_ref() == Some(&field_name))
                .map(|f| f.name().clone())
                .collect();

            Some(CompositeInfo::Root { children })
        } else if let Some(root) = field.composite_root() {
            Some(CompositeInfo::Member { root: root.clone() })
        } else {
            None
        }
    }

    fn check_field_name(&self, field_name: &Sym, local_name: &Option<Sym>) -> SchemaResult<()> {
        ensure!(
            self.parsed_fields
                .values()
                .find(|f| f.name == field_name || f.local_name == field_name)
                .is_none(),
            SchemaError::DuplicateField(field_name.into())
        );
        ensure!(
            is_valid_field_ident(field_name),
            FieldError::InvalidName.named(field_name)
        );
        if let Some(n) = local_name {
            ensure!(
                self.parsed_fields
                    .values()
                    .find(|f| f.name == n || f.local_name == n)
                    .is_none(),
                SchemaError::DuplicateField(n.into())
            );
            ensure!(
                is_valid_field_ident(n),
                FieldError::InvalidName.named(field_name)
            );
        }
        Ok(())
    }

    fn check_type_restrictions(&self, field: &RawFieldType) -> Result<(), FieldError> {
        let name = field.name();
        let kind = field.kind();
        let restriction = TypeRestriction::for_kind(kind);
        // could be `ensure!` but they got hard to read.
        if !restriction.can_dedupe_on && self.is_identity(name) {
            throw!(FieldError::BadTypeInDedupeOn(kind));
        }
        if restriction.forces_merge_strat && field.merge().is_some() {
            throw!(FieldError::TypeForbidsMergeStrat(kind));
        }
        if !restriction.valid_composite_member && field.composite_root().is_some() {
            throw!(FieldError::TypeNotComposite(kind));
        }
        Ok(())
    }

    fn check_composite_member_field(
        &self,
        field: &RawFieldType,
        merge: Option<ParsedMerge>,
    ) -> Result<(), FieldError> {
        let root = field
            .composite_root()
            .as_ref()
            .expect("Should check before calling");
        ensure!(merge.is_none(), FieldError::CompositeFieldMergeStrat);
        ensure!(
            self.input_fields.contains_key(root),
            FieldError::UnknownCompositeRoot(root.to_string())
        );
        Ok(())
    }

    fn check_composite_root_field(
        &self,
        field: &RawFieldType,
        merge: Option<ParsedMerge>,
    ) -> Result<(), FieldError> {
        let field_name = field.name();
        assert!(self.is_composite_root(field_name));
        ensure!(
            field.composite_root().is_none(),
            FieldError::CompositeRecursion
        );
        match merge {
            None
            | Some(ParsedMerge::TakeNewest)
            | Some(ParsedMerge::PreferRemote)
            | Some(ParsedMerge::TakeMin)
            | Some(ParsedMerge::TakeMax) => {
                // all good.
            }

            Some(other) => {
                throw!(FieldError::CompositeRootInvalidMergeStrat(other));
            }
        }
        Ok(())
    }

    fn check_used_features(&self, declared_features: &[Sym]) -> SchemaResult<()> {
        for (_, f) in &self.parsed_fields {
            if let FieldType::RecordSet { .. } = &f.ty {
                if !declared_features.contains(&"record_set".into()) {
                    throw!(SchemaError::UndeclaredFeatureRequired(
                        "record_set".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn get_field_type(&self, merge: ParsedMerge, field: &RawFieldType) -> SchemaResult<FieldType> {
        let field_name = field.name();
        let bad_merge = || {
            FieldError::IllegalMergeForType {
                ty: field.kind(),
                merge: merge.clone(),
            }
            .named(field_name)
        };
        Ok(match field {
            RawFieldType::Untyped { common } => {
                let merge = merge.to_untyped_merge(field).ok_or_else(bad_merge)?;
                FieldType::Untyped {
                    merge,
                    default: common.default.clone(),
                }
            }
            RawFieldType::Boolean { common } => {
                let merge = merge.to_boolean_merge(field).ok_or_else(bad_merge)?;
                FieldType::Boolean {
                    merge,
                    default: common.default,
                }
            }
            RawFieldType::Text { common } => {
                let merge = merge.to_text_merge(field).ok_or_else(bad_merge)?;
                FieldType::Text {
                    merge,
                    default: common.default.clone(),
                }
            }
            RawFieldType::Url { common, is_origin } => {
                let merge = merge.to_text_merge(field).ok_or_else(bad_merge)?;
                let default = if let Some(url) = &common.default {
                    let u = Url::parse(url)
                        .map_err(|e| FieldError::BadDefaultUrl(url.clone(), e))
                        .named(field_name)?;
                    if *is_origin && !valid_origin_only_url(&u) {
                        return Err(FieldError::BadDefaultOrigin(u.into_string()).named(field_name));
                    }
                    Some(u)
                } else {
                    None
                };
                FieldType::Url {
                    merge,
                    default,
                    is_origin: *is_origin,
                }
            }
            RawFieldType::Real {
                common,
                min,
                max,
                if_out_of_bounds,
            } => {
                let if_out_of_bounds = self
                    .check_number_bounds(field, min, max, if_out_of_bounds, &common.default)
                    .named(field_name)?;
                let merge = merge.to_number_merge(field).ok_or_else(bad_merge)?;
                FieldType::Real {
                    merge,
                    min: *min,
                    max: *max,
                    if_out_of_bounds: if_out_of_bounds.unwrap_or(IfOutOfBounds::Discard),
                    default: common.default,
                }
            }
            RawFieldType::Integer {
                common,
                min,
                max,
                if_out_of_bounds,
            } => {
                let if_out_of_bounds = self
                    .check_number_bounds(field, min, max, if_out_of_bounds, &common.default)
                    .named(field_name)?;
                let merge = merge.to_number_merge(field).ok_or_else(bad_merge)?;
                FieldType::Integer {
                    merge,
                    min: *min,
                    max: *max,
                    if_out_of_bounds: if_out_of_bounds.unwrap_or(IfOutOfBounds::Discard),
                    default: common.default,
                }
            }
            RawFieldType::Timestamp { common, semantic } => {
                let merge = merge
                    .clone()
                    .to_timestamp_merge(field)
                    .ok_or_else(bad_merge)?;
                self.get_timestamp_field(merge, common, semantic)
                    .named(field_name)?
            }
            RawFieldType::OwnGuid { auto, .. } => FieldType::OwnGuid {
                auto: auto.unwrap_or(true),
            },
            RawFieldType::UntypedMap {
                common,
                prefer_deletions,
            } => FieldType::UntypedMap {
                prefer_deletions: *prefer_deletions,
                default: common.default.clone(),
            },
            RawFieldType::RecordSet {
                common,
                id_key,
                prefer_deletions,
            } => self
                .get_record_set_field(common, id_key, *prefer_deletions)
                .named(field_name)?,
        })
    }

    fn get_timestamp_field(
        &self,
        merge: TimestampMerge,
        common: &RawFieldCommon<Option<RawTimeDefault>>,
        semantic: &Option<RawTimestampSemantic>,
    ) -> Result<FieldType, FieldError> {
        let semantic = if let Some(sem) = semantic.as_ref().and_then(TimestampSemantic::from_raw) {
            let want = sem.required_merge();
            ensure!(
                merge == want,
                FieldError::BadMergeForTimestampSemantic {
                    sem,
                    want,
                    got: merge,
                }
            );
            Some(sem)
        } else {
            None
        };
        let tsd: Option<TimestampDefault> = common
            .default
            .as_ref()
            .map(|d| match d {
                RawTimeDefault::Num(v) => Ok(TimestampDefault::Value(*v)),
                RawTimeDefault::Special(RawSpecialTime::Now) => Ok(TimestampDefault::Now),
                RawTimeDefault::Special(RawSpecialTime::Unknown(v)) => Err(
                    FieldError::UnknownVariant(format!("unknown special timestamp value: {}", v)),
                ),
            })
            .transpose()?;

        if let Some(TimestampDefault::Value(default)) = tsd {
            ensure!(
                default >= crate::ms_time::EARLIEST_SANE_TIME,
                FieldError::DefaultTimestampTooOld,
            );
        }
        Ok(FieldType::Timestamp {
            merge,
            semantic,
            default: tsd,
        })
    }

    fn get_record_set_field(
        &self,
        common: &RawFieldCommon<Option<Vec<JsonObject>>>,
        id_key: &str,
        prefer_deletions: bool,
    ) -> Result<FieldType, FieldError> {
        if let Some(s) = &common.default {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for r in s {
                let id = r.get(id_key).ok_or_else(|| {
                    FieldError::BadRecordSetDefault(BadRecordSetDefaultKind::IdKeyMissing)
                })?;
                if let JsonValue::String(s) = id {
                    ensure!(
                        !seen.contains(s.as_str()),
                        FieldError::BadRecordSetDefault(BadRecordSetDefaultKind::IdKeyDupe),
                    );
                    seen.insert(s);
                } else {
                    // We could probably allow numbers...
                    throw!(FieldError::BadRecordSetDefault(
                        BadRecordSetDefaultKind::IdKeyInvalidType
                    ));
                }
            }
        }
        Ok(FieldType::RecordSet {
            default: common.default.clone(),
            id_key: id_key.into(),
            prefer_deletions,
        })
    }

    fn check_number_bounds<T: Copy + PartialOrd + BoundedNum>(
        &self,
        field: &RawFieldType,
        min: &Option<T>,
        max: &Option<T>,
        if_oob: &Option<RawIfOutOfBounds>,
        default: &Option<T>, // f: &RawFieldType
    ) -> Result<Option<IfOutOfBounds>, FieldError> {
        let valid_if_oob = if_oob
            .as_ref()
            .map(|v| {
                IfOutOfBounds::from_raw(v).ok_or_else(|| {
                    FieldError::UnknownVariant(format!("unknown if_out_of_bounds value: {:?}", v))
                })
            })
            .transpose()?;
        ensure!(
            min.map_or(true, |v| v.sane_value()),
            FieldError::BadNumBounds,
        );
        ensure!(
            max.map_or(true, |v| v.sane_value()),
            FieldError::BadNumBounds,
        );
        if min.is_some() || max.is_some() {
            ensure!(if_oob.is_some(), FieldError::NoBoundsCheckInfo);
        }
        ensure!(
            !matches!((min, max), (Some(lo), Some(hi)) if hi < lo),
            FieldError::BadNumBounds,
        );
        if max.is_some() {
            ensure!(
                field.get_merge()? != Some(ParsedMerge::TakeSum),
                FieldError::MergeTakeSumNoMax,
            );
        }

        if let Some(d) = default {
            let min = min.unwrap_or(T::min_max_defaults().0);
            let max = max.unwrap_or(T::min_max_defaults().1);
            ensure!(min <= *d && *d <= max, FieldError::BadNumDefault);
        }
        Ok(valid_if_oob)
    }
}

// way to avoid having to duplicate the f64 bound handing stuff for i64
trait BoundedNum: Sized {
    fn sane_value(self) -> bool;
    fn min_max_defaults() -> (Self, Self);
}

impl BoundedNum for f64 {
    fn sane_value(self) -> bool {
        !self.is_nan() && !self.is_infinite()
    }
    fn min_max_defaults() -> (Self, Self) {
        (std::f64::NEG_INFINITY, std::f64::INFINITY)
    }
}

impl BoundedNum for i64 {
    fn sane_value(self) -> bool {
        true
    }
    fn min_max_defaults() -> (Self, Self) {
        (std::i64::MIN, std::i64::MAX)
    }
}

fn is_valid_field_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 128
        && s.is_ascii()
        && s.bytes()
            .all(|b| b == b'$' || crate::util::is_base64url_byte(b))
}

fn valid_origin_only_url(u: &Url) -> bool {
    !u.has_authority()
        && !u.cannot_be_a_base()
        && u.path() == "/"
        && u.query().is_none()
        && u.fragment().is_none()
}

#[derive(Debug, Clone, PartialEq)]
struct TypeRestriction {
    can_dedupe_on: bool,
    valid_composite_member: bool,
    forces_merge_strat: bool,
}

impl TypeRestriction {
    fn new(can_dedupe_on: bool, valid_composite_member: bool, forces_merge_strat: bool) -> Self {
        Self {
            can_dedupe_on,
            valid_composite_member,
            forces_merge_strat,
        }
    }
    fn permit_all() -> Self {
        Self::new(true, true, false)
    }

    fn forbid_all() -> Self {
        Self::new(false, false, true)
    }

    fn for_kind(k: FieldKind) -> Self {
        match k {
            FieldKind::Untyped => TypeRestriction::permit_all(),
            FieldKind::Text => TypeRestriction::permit_all(),
            FieldKind::Url => TypeRestriction::permit_all(),
            FieldKind::Integer => TypeRestriction::new(false, true, false),
            FieldKind::Timestamp => TypeRestriction::new(false, true, false),
            FieldKind::Real => TypeRestriction::new(false, true, false),
            FieldKind::Boolean => TypeRestriction::permit_all(),

            FieldKind::OwnGuid => TypeRestriction::forbid_all(),
            FieldKind::UntypedMap => TypeRestriction::forbid_all(),
            FieldKind::RecordSet => TypeRestriction::forbid_all(),
        }
    }
}

define_enum_with_unknown! {
    #[derive(Clone, Debug, PartialEq, PartialOrd)]
    pub enum RawIfOutOfBounds {
        Clamp = "clamp",
        Discard = "discard",
        .. Unknown(crate::Sym),
    }
    IMPL_FROM_RAW = IfOutOfBounds;
}

define_enum_with_unknown! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum ParsedMerge {
        TakeNewest = "take_newest",
        PreferRemote = "prefer_remote",
        Duplicate = "duplicate",
        TakeMin = "take_min",
        TakeMax = "take_max",
        TakeSum = "take_sum",
        PreferFalse = "prefer_false",
        PreferTrue = "prefer_true",
        .. Unknown(crate::Sym),
    }
    DERIVE_DISPLAY = true;
}

impl ParsedMerge {
    fn to_untyped_merge(&self, f: &RawFieldType) -> Option<UntypedMerge> {
        if f.composite_root().is_some() {
            return Some(UntypedMerge::CompositeMember);
        }
        match self {
            ParsedMerge::TakeNewest => Some(UntypedMerge::TakeNewest),
            ParsedMerge::PreferRemote => Some(UntypedMerge::PreferRemote),
            ParsedMerge::Duplicate => Some(UntypedMerge::Duplicate),
            _ => None,
        }
    }

    fn to_text_merge(&self, f: &RawFieldType) -> Option<TextMerge> {
        Some(TextMerge::Untyped(self.to_untyped_merge(f)?))
    }

    fn to_number_merge(&self, f: &RawFieldType) -> Option<NumberMerge> {
        if let Some(u) = self.to_untyped_merge(f) {
            Some(NumberMerge::Untyped(u))
        } else {
            match self {
                ParsedMerge::TakeMin => Some(NumberMerge::TakeMin),
                ParsedMerge::TakeMax => Some(NumberMerge::TakeMax),
                ParsedMerge::TakeSum => Some(NumberMerge::TakeSum),
                _ => None,
            }
        }
    }

    fn to_timestamp_merge(&self, f: &RawFieldType) -> Option<TimestampMerge> {
        if let Some(u) = self.to_untyped_merge(f) {
            Some(TimestampMerge::Untyped(u))
        } else {
            match self {
                ParsedMerge::TakeMin => Some(TimestampMerge::TakeMin),
                ParsedMerge::TakeMax => Some(TimestampMerge::TakeMax),
                _ => None,
            }
        }
    }

    fn to_boolean_merge(&self, f: &RawFieldType) -> Option<BooleanMerge> {
        if let Some(u) = self.to_untyped_merge(f) {
            Some(BooleanMerge::Untyped(u))
        } else {
            match self {
                ParsedMerge::PreferTrue => Some(BooleanMerge::PreferTrue),
                ParsedMerge::PreferFalse => Some(BooleanMerge::PreferFalse),
                _ => None,
            }
        }
    }
}
