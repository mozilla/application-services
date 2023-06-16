// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{fmt::Display, path::Path};

use anyhow::Result;
use console::Term;
use serde_json::Value;

use crate::{
    sources::{ExperimentListSource, ExperimentSource},
    value_utils::{self, CliUtils},
    NimbusApp,
};

#[derive(serde::Serialize, Debug)]
struct ExperimentInfo<'a> {
    slug: &'a str,
    app_name: &'a str,
    channel: &'a str,
    branches: Vec<&'a str>,
    features: Vec<&'a str>,
    targeting: &'a str,
    bucketing: u64,
    is_rollout: bool,
    user_facing_name: &'a str,
    user_facing_description: &'a str,
    enrollment: DateRange<'a>,
    is_enrollment_paused: bool,
    duration: DateRange<'a>,
}

impl ExperimentInfo<'_> {
    fn bucketing_percent(&self) -> String {
        format!("{: >3.0} %", self.bucketing / 100)
    }
}

#[derive(serde::Serialize, Debug)]
struct DateRange<'a> {
    start: Option<&'a str>,
    end: Option<&'a str>,
    proposed: Option<i64>,
}

impl<'a> DateRange<'a> {
    fn new(start: Option<&'a Value>, end: Option<&'a Value>, duration: Option<&'a Value>) -> Self {
        let start = start.map(Value::as_str).unwrap_or_default();
        let end = end.map(Value::as_str).unwrap_or_default();
        let proposed = duration.map(Value::as_i64).unwrap_or_default();
        Self {
            start,
            end,
            proposed,
        }
    }
}

impl Display for DateRange<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.start, self.end, self.proposed) {
            (Some(s), Some(e), _) => f.write_str(&format!("{s} ➞ {e}")),
            (Some(s), _, Some(d)) => f.write_str(&format!("{s}, proposed ending after {d} days")),
            (Some(s), _, _) => f.write_str(&format!("{s} ➞ ?")),
            (None, Some(e), Some(d)) => {
                f.write_str(&format!("ending {e}, started {d} days before"))
            }
            (None, Some(e), _) => f.write_str(&format!("ending {e}")),
            _ => f.write_str("unknown"),
        }
    }
}

impl<'a> TryFrom<&'a Value> for ExperimentInfo<'a> {
    type Error = anyhow::Error;

    fn try_from(exp: &'a Value) -> Result<Self> {
        let features: Vec<_> = exp
            .get_array("featureIds")?
            .iter()
            .flat_map(|f| f.as_str())
            .collect();
        let branches: Vec<_> = exp
            .get_array("branches")?
            .iter()
            .flat_map(|b| {
                b.get("slug")
                    .expect("Expecting a branch with a slug")
                    .as_str()
            })
            .collect();

        let config = exp.get_object("bucketConfig")?;

        Ok(Self {
            slug: exp.get_str("slug")?,
            app_name: exp.get_str("appName")?,
            channel: exp.get_str("channel")?,
            branches,
            features,
            targeting: exp.get_str("targeting")?,
            bucketing: config.get_u64("count")?,
            is_rollout: exp.get_bool("isRollout")?,
            user_facing_name: exp.get_str("userFacingName")?,
            user_facing_description: exp.get_str("userFacingDescription")?,
            enrollment: DateRange::new(
                exp.get("startDate"),
                exp.get("enrollmentEndDate"),
                exp.get("proposedEnrollment"),
            ),
            is_enrollment_paused: exp.get_bool("isEnrollmentPaused")?,
            duration: DateRange::new(
                exp.get("startDate"),
                exp.get("endDate"),
                exp.get("proposedDuration"),
            ),
        })
    }
}

impl ExperimentListSource {
    pub(crate) fn print_list(&self, params: &NimbusApp) -> Result<bool> {
        let value: Value = self.try_into()?;
        let array = value_utils::try_extract_data_list(&value)?;

        let term = Term::stdout();
        let style = term.style().italic().underlined();
        term.write_line(&format!(
            "{slug: <66}|{channel: <9}|{bucketing: >7}|{features: <31}|{is_rollout}|{branches: <20}",
            slug = style.apply_to("Experiment slug"),
            channel = style.apply_to(" Channel"),
            bucketing = style.apply_to(" % "),
            features = style.apply_to(" Features"),
            is_rollout = style.apply_to("   "),
            branches = style.apply_to(" Branches"),
        ))?;
        for exp in array {
            let app_name = exp.get_str("appName").ok().unwrap_or_default();
            if app_name != params.app_name {
                continue;
            }

            let info = match ExperimentInfo::try_from(&exp) {
                Ok(e) => e,
                _ => continue,
            };

            let is_rollout = if info.is_rollout { "R" } else { "" };

            term.write_line(&format!(
                " {slug: <65}| {channel: <8}| {bucketing: >5} | {features: <30}| {is_rollout: <1} | {branches}",
                slug = info.slug,
                channel = info.channel,
                bucketing = info.bucketing_percent(),
                features = info.features.join(", "),
                branches = info.branches.join(", ")
            ))?;
        }
        Ok(true)
    }
}

impl ExperimentSource {
    pub(crate) fn print_info<P>(&self, output: Option<P>) -> Result<bool>
    where
        P: AsRef<Path>,
    {
        let value = self.try_into()?;
        let info: ExperimentInfo = ExperimentInfo::try_from(&value)?;
        if output.is_some() {
            value_utils::write_to_file_or_print(output, &info)?;
            return Ok(true);
        }
        let url = match self {
            Self::FromApiV6 { slug, endpoint } => Some(format!("{endpoint}/nimbus/{slug}/summary")),
            _ => None,
        };
        let term = Term::stdout();
        let t_style = term.style().italic();
        let d_style = term.style().bold().cyan();
        let line = |title: &str, detail: &str| {
            _ = term.write_line(&format!(
                "{: <11} {}",
                t_style.apply_to(title),
                d_style.apply_to(detail)
            ));
        };

        let enrollment = format!(
            "{} ({})",
            info.enrollment,
            if info.is_enrollment_paused {
                "paused"
            } else {
                "enrolling"
            }
        );

        let is_rollout = if info.is_rollout {
            "Rollout".to_string()
        } else {
            let n = info.branches.len();
            let b = if n == 1 {
                "1 branch".to_string()
            } else {
                format!("{n} branches")
            };
            format!("Experiment with {b}")
        };

        line("Slug", info.slug);
        line("Name", info.user_facing_name);
        line("Description", info.user_facing_description);
        if let Some(url) = url {
            line("URL", &url);
        }
        line("App", info.app_name);
        line("Channel", info.channel);
        line("E/R", &is_rollout);
        line("Enrollment", &enrollment);
        line("Observing", &info.duration.to_string());
        line("Targeting", &format!("\"{}\"", info.targeting));
        line("Bucketing", &info.bucketing_percent());
        line("Branches", &info.branches.join(", "));
        line("Features", &info.features.join(", "));

        Ok(true)
    }
}
