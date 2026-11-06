/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Custom lints for Nimbus feature manifests.
//!
//! Where validation asks whether a manifest can generate working code, these ask
//! whether the feature is one an experiment owner can work with. They never stop
//! code generation, and they're reported by `nimbus-fml lint` alone.
//!
//! A lint can be silenced by a `no-lint` list on a feature.

/// Register the lints for a module.
macro_rules! define_lints {
    ($($name:ident: $category:ident = $description:literal, $help:literal;)*) => {
        $(
            pub static $name: $crate::lints::LintInfo = $crate::lints::LintInfo {
                name: stringify!($name),
                description: $description,
                help: $help,
                category: $crate::lints::LintCategory::$category,
            };
        )*

        pub static LINTS: &[&$crate::lints::LintInfo] = &[$(&$name),*];
    };
}

/// Declares [`LintCategory`] together with the [`Linter`] owning each category, so a
/// module of lints can't be declared without being registered: [`define_lints!`]
/// can't name a category that doesn't exist, and a category can't exist here without
/// a linter to run.
macro_rules! define_categories {
    ($($(#[$doc:meta])* $variant:ident = $name:literal => $linter:expr;)*) => {
        /// What a lint is about, as reported by `nimbus-fml lint --list`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
        pub enum LintCategory {
            $(
                $(#[$doc])*
                #[serde(rename = $name)]
                $variant,
            )*
        }

        impl LintCategory {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }

        /// Every module of lints, in `--list` order.
        static LINTERS: &[&dyn Linter] = &[$(&$linter),*];
    };
}

mod design;
mod documentation;
mod metadata;
mod naming;

use std::collections::HashSet;

use serde::Serialize;

use crate::intermediate_representation::{
    EnumDef, FeatureDef, FeatureManifest, ObjectDef, PropDef, TypeRef,
};

define_lints! {
    UNKNOWN_LINT: Lints =
        "A `no-lint` entry names a lint that doesn't exist.",
        "Run `nimbus-fml lint --list` to see the available lints.";
}

/// The lints of one [`LintCategory`]. Every check has a do-nothing default, so an
/// implementation writes only the ones its lints have something to say about.
pub(crate) trait Linter: Sync {
    /// The `LINTS` slice [`define_lints!`] generated for the module.
    fn lints(&self) -> &'static [&'static LintInfo];

    fn check_feature(
        &self,
        _feature: &FeatureDef,
        _manifest: &FeatureManifest,
        _out: &mut Vec<RawFinding>,
    ) {
    }

    fn check_object(&self, _object: &ObjectDef, _out: &mut Vec<RawFinding>) {}

    fn check_enum(&self, _enum_def: &EnumDef, _out: &mut Vec<RawFinding>) {}

    /// Run once per manifest, for lints which need to see every feature at once.
    fn check_manifest(&self, _manifest: &FeatureManifest, _out: &mut Vec<RawFinding>) {}
}

define_categories! {
    Metadata = "metadata" => metadata::Metadata;
    Documentation = "documentation" => documentation::Documentation;
    Naming = "naming" => naming::Naming;
    Design = "design" => design::Design;
    /// Lints about the lints themselves.
    Lints = "lints" => Lints;
}

/// The file level `no-lint` list is checked by [`lint_manifest`] instead, since it
/// belongs to no feature.
struct Lints;

impl Linter for Lints {
    fn lints(&self) -> &'static [&'static LintInfo] {
        LINTS
    }

    fn check_feature(
        &self,
        feature: &FeatureDef,
        _manifest: &FeatureManifest,
        out: &mut Vec<RawFinding>,
    ) {
        check_no_lint_names(
            feature.metadata.no_lint.iter().map(String::as_str),
            feature_path(feature),
            out,
        );
    }
}

#[derive(Debug)]
pub struct LintInfo {
    pub name: &'static str,
    pub description: &'static str,
    /// What to do about it. Shown once per report, not once per finding.
    pub help: &'static str,
    pub category: LintCategory,
}

lazy_static::lazy_static! {
    /// Every lint, in `--list` order.
    pub static ref ALL_LINTS: Vec<&'static LintInfo> = LINTERS
        .iter()
        .flat_map(|linter| linter.lints())
        .copied()
        .collect();
}

pub fn find_lint(name: &str) -> Option<&'static LintInfo> {
    ALL_LINTS.iter().find(|l| l.name == name).copied()
}

/// Where in the manifest a finding is. The `subject` is what findings are grouped
/// under when reported, so it is the feature, object or enum, never a member of one.
#[derive(Debug, Clone)]
pub(crate) struct Location {
    subject: String,
    member: Option<String>,
}

impl Location {
    fn subject(subject: String) -> Self {
        Self {
            subject,
            member: None,
        }
    }

    fn member(subject: String, member: String) -> Self {
        Self {
            subject,
            member: Some(member),
        }
    }

    pub(crate) fn is_member(&self) -> bool {
        self.member.is_some()
    }
}

/// A finding before the runner has decided whether to report it.
#[derive(Debug, Clone)]
pub(crate) struct RawFinding {
    lint: &'static LintInfo,
    location: Location,
    message: String,
}

impl RawFinding {
    pub(crate) fn new(
        lint: &'static LintInfo,
        location: Location,
        message: impl Into<String>,
    ) -> RawFinding {
        Self {
            lint,
            location,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub lint: &'static str,
    /// Set if the finding came from an imported manifest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// e.g. ``feature `homescreen` ``.
    pub subject: String,
    /// e.g. ``variable `enabled` ``.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct LintReport {
    pub findings: Vec<Finding>,
    /// How many findings `no-lint` silenced, so that a manifest can't opt out of
    /// everything and look clean.
    pub suppressed: usize,
}

impl LintReport {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn finding_count(&self) -> usize {
        self.findings.len()
    }

    /// How many features, objects and enums have findings.
    pub fn subject_count(&self) -> usize {
        self.findings
            .iter()
            .map(|f| (&f.module, &f.subject))
            .collect::<HashSet<_>>()
            .len()
    }

    /// The lints that fired, in registration order.
    pub fn triggered_lints(&self) -> Vec<&'static LintInfo> {
        let fired: HashSet<_> = self.findings.iter().map(|f| f.lint).collect();
        ALL_LINTS
            .iter()
            .filter(|l| fired.contains(l.name))
            .copied()
            .collect()
    }
}

/// Run the lints against a manifest that has already been through
/// [`FeatureManifest::validate_manifest`].
pub fn lint_manifest(fm: &FeatureManifest) -> LintReport {
    let mut report = LintReport::default();

    lint_module(fm, None, &mut report);

    for (id, child) in &fm.all_imports {
        lint_module(child, Some(id.to_string()), &mut report);
    }

    // Group issues by location.
    report.findings.sort_by(|a, b| {
        (&a.module, &a.subject, a.lint, &a.member).cmp(&(&b.module, &b.subject, b.lint, &b.member))
    });

    report
}

fn lint_module(fm: &FeatureManifest, module: Option<String>, out: &mut LintReport) {
    for feature in fm.iter_feature_defs() {
        let mut raw = Vec::new();
        for linter in LINTERS {
            linter.check_feature(feature, fm, &mut raw);
        }

        let allowed_lints: HashSet<&str> = feature
            .metadata
            .no_lint
            .iter()
            .map(String::as_str)
            .collect();
        collect_findings(raw, &allowed_lints, module.as_deref(), out);
    }

    let allowed_lints = HashSet::new();

    for object in fm.iter_object_defs() {
        let mut raw = Vec::new();
        for linter in LINTERS {
            linter.check_object(object, &mut raw);
        }
        collect_findings(raw, &allowed_lints, module.as_deref(), out);
    }

    for enum_def in fm.iter_enum_defs() {
        let mut raw = Vec::new();
        for linter in LINTERS {
            linter.check_enum(enum_def, &mut raw);
        }
        collect_findings(raw, &allowed_lints, module.as_deref(), out);
    }

    let mut raw = Vec::new();
    for linter in LINTERS {
        linter.check_manifest(fm, &mut raw);
    }
    collect_findings(raw, &allowed_lints, module.as_deref(), out);
}

fn collect_findings(
    raw: Vec<RawFinding>,
    allowed_lints: &HashSet<&str>,
    module: Option<&str>,
    out: &mut LintReport,
) {
    for finding in raw {
        let name = finding.lint.name;

        if allowed_lints.contains(name) {
            out.suppressed += 1;
            continue;
        }

        out.findings.push(Finding {
            lint: name,
            module: module.map(Into::into),
            subject: finding.location.subject,
            member: finding.location.member,
            message: finding.message,
        });
    }
}

fn check_no_lint_names<'a>(
    names: impl IntoIterator<Item = &'a str>,
    location: Location,
    out: &mut Vec<RawFinding>,
) {
    for name in names {
        if find_lint(name).is_none() {
            out.push(RawFinding::new(
                &UNKNOWN_LINT,
                location.clone(),
                format!("`no-lint` names `{name}`, which isn't a lint"),
            ));
        }
    }
}

/// `Option<Boolean>`, as gecko-pref backed variables use, is still a boolean.
pub(crate) fn is_boolean(typ: &TypeRef) -> bool {
    match typ {
        TypeRef::Boolean => true,
        TypeRef::Option(inner) => is_boolean(inner),
        _ => false,
    }
}

pub(crate) fn feature_path(feature: &FeatureDef) -> Location {
    Location::subject(format!("feature `{}`", feature.name))
}

pub(crate) fn variable_path(feature: &FeatureDef, prop: &PropDef) -> Location {
    Location::member(
        format!("feature `{}`", feature.name),
        format!("variable `{}`", prop.name),
    )
}

pub(crate) fn object_path(object: &ObjectDef) -> Location {
    Location::subject(format!("object `{}`", object.name))
}

pub(crate) fn object_field_path(object: &ObjectDef, prop: &PropDef) -> Location {
    Location::member(
        format!("object `{}`", object.name),
        format!("field `{}`", prop.name),
    )
}

pub(crate) fn enum_path(enum_def: &EnumDef) -> Location {
    Location::subject(format!("enum `{}`", enum_def.name))
}

pub(crate) fn enum_variant_path(enum_def: &EnumDef, variant: &str) -> Location {
    Location::member(
        format!("enum `{}`", enum_def.name),
        format!("variant `{variant}`"),
    )
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::intermediate_representation::{FeatureDef, PropDef, TypeRef};
    use serde_json::json;

    #[test]
    fn test_lint_names_are_unique_and_well_formed() {
        let mut seen = HashSet::new();
        for lint in ALL_LINTS.iter() {
            assert!(
                seen.insert(lint.name),
                "{} is registered more than once",
                lint.name
            );
            assert!(
                lint.name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                "{} isn't SCREAMING_SNAKE_CASE",
                lint.name
            );
            assert!(
                lint.description.ends_with('.'),
                "{}'s description should be a sentence",
                lint.name
            );
            assert!(
                lint.help.ends_with('.'),
                "{}'s help should be a sentence",
                lint.name
            );
        }
    }

    /// `define_categories!` makes owning a category the only way to be run, so what
    /// is left to check is that no two modules share one.
    #[test]
    fn test_each_linter_owns_one_category() {
        let mut owned = HashSet::new();

        for linter in LINTERS {
            let declared = linter.lints();
            assert!(!declared.is_empty(), "a linter declares no lints");

            let categories: HashSet<_> = declared.iter().map(|l| l.category).collect();
            assert_eq!(
                categories.len(),
                1,
                "{:?} spans more than one category",
                declared.iter().map(|l| l.name).collect::<Vec<_>>()
            );

            assert!(
                owned.insert(declared[0].category),
                "two linters declare {} lints; one of them is never run",
                declared[0].category.as_str()
            );
        }
    }

    /// No metadata, no description, no variables.
    fn empty_feature(name: &str) -> FeatureDef {
        FeatureDef::new(name, "", Default::default(), false)
    }

    #[test]
    fn test_every_lint_is_registered() {
        for lint in ALL_LINTS.iter() {
            assert_eq!(find_lint(lint.name).map(|l| l.name), Some(lint.name));
        }
    }

    #[test]
    fn test_suppressed_findings_are_counted() {
        let mut suppressed = empty_feature("suppressed");
        suppressed.metadata.no_lint = vec!["NO_VARIABLES".to_string()];

        let mut fm = FeatureManifest::default();
        fm.add_feature(suppressed);

        let report = lint_manifest(&fm);
        assert_eq!(report.suppressed, 1);
    }

    #[test]
    fn test_findings_are_reported_once_per_lint() {
        let mut fm = FeatureManifest::default();
        fm.add_feature(empty_feature("my-feature"));

        let report = lint_manifest(&fm);
        let mut lints: Vec<_> = report.findings.iter().map(|f| f.lint).collect();
        lints.sort_unstable();
        let deduped = lints.iter().collect::<HashSet<_>>();
        assert_eq!(lints.len(), deduped.len());

        assert!(lints.contains(&"NO_VARIABLES"));
        assert!(lints.contains(&"MISSING_DESCRIPTION"));
        assert!(lints.contains(&"MISSING_CONTACTS"));
    }

    #[test]
    fn test_feature_suppressions() {
        let mut suppressed = empty_feature("suppressed");
        suppressed.metadata.no_lint = vec!["NO_VARIABLES".to_string()];

        let mut fm = FeatureManifest::default();
        fm.add_feature(suppressed);
        fm.add_feature(empty_feature("not-suppressed"));

        let report = lint_manifest(&fm);
        let features: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.lint == "NO_VARIABLES")
            .map(|f| f.subject.as_str())
            .collect();
        assert_eq!(features, vec!["feature `not-suppressed`"]);
    }

    #[test]
    fn test_unknown_lint_names_in_the_manifest_are_reported() {
        let mut feature = empty_feature("my-feature");
        feature.metadata.no_lint = vec!["NOT_A_LINT".to_string()];

        let mut fm = FeatureManifest::default();
        fm.add_feature(feature);

        let report = lint_manifest(&fm);
        assert!(report
            .findings
            .iter()
            .any(|f| f.lint == "UNKNOWN_LINT" && f.message.contains("NOT_A_LINT")));
    }

    #[test]
    fn test_a_well_formed_feature_is_quiet() {
        let mut feature = FeatureDef::new(
            "my-feature",
            "Controls the shape and behaviour of the widget on the home screen.",
            vec![
                PropDef::with_doc(
                    "enabled",
                    "Whether the widget is shown on the home screen at all.",
                    &TypeRef::Boolean,
                    &json!(false),
                ),
                PropDef::with_doc(
                    "max-rows",
                    "The largest number of rows the widget is allowed to grow to.",
                    &TypeRef::Int,
                    &json!(3),
                ),
            ],
            false,
        );
        feature.metadata.contacts = vec!["email@example.com".into()];
        let mut fm = FeatureManifest::default();
        fm.add_feature(feature);

        // Metadata lints are expected; nothing else should fire.
        let report = lint_manifest(&fm);
        assert!(
            report.is_empty(),
            "unexpected findings: {:?}",
            report.findings
        );
    }
}
