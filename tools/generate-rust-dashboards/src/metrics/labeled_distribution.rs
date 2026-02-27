/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, LabeledDistributionMetric, ReleaseChannel, TeamConfig},
    schema::{
        DashboardBuilder, Datasource, FieldConfig, FieldConfigCustom, FieldConfigDefaults, GridPos,
        Panel, ScaleDistribution, Target, TimeSeriesPanel, Transformation,
    },
    sql::Query,
    Result,
};

pub fn add_to_dashboard(
    builder: &mut DashboardBuilder,
    _config: &TeamConfig,
    metric: &LabeledDistributionMetric,
) -> Result<()> {
    builder.add_panel_title(metric.display_name);
    for app in metric.applications.iter().cloned() {
        for channel in ReleaseChannel::all() {
            builder.add_panel_third(count_panel(app, channel, metric, "q05", "5th percentile"));
            builder.add_panel_third(count_panel(app, channel, metric, "q50", "50th percentile"));
            builder.add_panel_third(count_panel(app, channel, metric, "q95", "95th percentile"));
        }
    }
    Ok(())
}

fn count_panel(
    application: Application,
    channel: ReleaseChannel,
    metric: &LabeledDistributionMetric,
    quantile: &str,
    quantile_label: &str,
) -> Panel {
    let channel = channel.slug();
    let LabeledDistributionMetric {
        ping,
        category,
        metric,
        value_divisor,
        axis_label,
        unit,
        ..
    } = *metric;
    let query = Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            "label".into(),
            match value_divisor {
                None => format!("{quantile} as amount"),
                Some(amount) => format!("{quantile} / {amount} as amount"),
            },
        ],
        from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
        order_by: Some("submission_date asc".into()),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            "label IS NOT NULL".into(),
            format!("channel = '{channel}'"),
        ],
        ..Query::default()
    };

    TimeSeriesPanel {
        title: format!("{application} - {channel} ({quantile_label})"),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![],
                custom: FieldConfigCustom {
                    axis_label: axis_label.into(),
                    scale_distribution: ScaleDistribution {
                        type_: "log".into(),
                        log: Some(10),
                    },
                    ..FieldConfigCustom::default()
                },
                unit,
            },
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["label".into()],
                keep_fields: true,
            },
            // Fixup the field names for better legend labels
            Transformation::RenameByRegex {
                regex: "amount (.*)".into(),
                rename_pattern: "$1".into(),
            },
        ],
        ..TimeSeriesPanel::default()
    }
    .into()
}
