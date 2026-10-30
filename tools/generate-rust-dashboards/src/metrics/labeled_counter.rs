/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, LabeledCounterMetric, ReleaseChannel, TeamConfig},
    schema::{
        DashboardBuilder, Datasource, FieldConfig, FieldConfigCustom, FieldConfigDefaults,
        FieldConfigOverride, FieldConfigOverrideMatcher, GridPos, Panel, Target, TimeSeriesPanel,
        Transformation,
    },
    sql::{Query, Union},
    util::dashboard_count_color,
    Result,
};

pub fn add_to_dashboard(
    builder: &mut DashboardBuilder,
    _config: &TeamConfig,
    metric: &LabeledCounterMetric,
) -> Result<()> {
    builder.add_panel_title(metric.display_name);
    for app in metric.applications.iter().cloned() {
        builder.add_panel_third(count_panel(app, ReleaseChannel::Nightly, metric));
        builder.add_panel_third(count_panel(app, ReleaseChannel::Beta, metric));
        builder.add_panel_third(count_panel(app, ReleaseChannel::Release, metric));
    }
    Ok(())
}

fn count_panel(
    application: Application,
    channel: ReleaseChannel,
    metric: &LabeledCounterMetric,
) -> Panel {
    // Note: some of this code is untested since we don't have any counters in use right now

    let LabeledCounterMetric {
        ping,
        category,
        metric,
        options,
        ..
    } = metric;

    let mut union = Union::default();
    let base_query = Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            "label".into(),
            "count".into(),
        ],
        from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            format!("application = `{}`", application.slug()),
            format!("channel = '{channel}'"),
            "label IS NOT NULL".into(),
        ],
        order_by: Some("submission_date asc".into()),
        ..Query::default()
    };

    union.queries.push(Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            "label".into(),
            "count".into(),
        ],
        ..base_query.clone()
    });

    if options.unique_user_counts {
        union.queries.push(Query {
            select: vec![
                "TIMESTAMP(submission_date) as time".into(),
                "CONCAT(label, ' (daily unique users)')".into(),
                "client_count AS count".into(),
            ],
            ..base_query.clone()
        });
    }

    let mut color_overrides = vec![];

    if let Some(labels) = options.labels.as_ref() {
        for (i, label) in labels.iter().enumerate() {
            color_overrides.push(FieldConfigOverride {
                matcher: FieldConfigOverrideMatcher {
                    id: "byName".into(),
                    options: label.to_string(),
                },
                properties: vec![dashboard_count_color(i, false)],
            });
            if options.unique_user_counts {
                color_overrides.push(FieldConfigOverride {
                    matcher: FieldConfigOverrideMatcher {
                        id: "byName".into(),
                        options: format!("{label} (daily unique users)"),
                    },
                    properties: vec![dashboard_count_color(i, true)],
                });
            }
        }
    }

    TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
        targets: vec![Target::table(union.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![],
                custom: FieldConfigCustom {
                    axis_label: "count / day".into(),
                    ..FieldConfigCustom::default()
                },
                unit: None,
            },
            ..FieldConfig::default()
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["label".into()],
                keep_fields: true,
            },
            // Fixup the field names for better legend labels
            Transformation::RenameByRegex {
                regex: "count (.*)".into(),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    }
    .into()
}
