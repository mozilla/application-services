/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, LabeledCounterMetric, ReleaseChannel, TeamConfig},
    schema::{
        DashboardBuilder, Datasource, FieldConfig, FieldConfigCustom, FieldConfigDefaults, GridPos,
        Panel, Target, TimeSeriesPanel, Transformation,
    },
    sql::Query,
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
    let LabeledCounterMetric {
        ping,
        category,
        metric,
        ..
    } = *metric;
    let dataset = application.bigquery_dataset();

    let mut query = Query {
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "counter.key as label".into(),
        ],
        from: format!("`mozdata.{dataset}.{ping}`"),
        joins: vec![format!(
            "CROSS JOIN UNNEST(metrics.labeled_counter.{category}_{metric}) as counter"
        )],
        where_: vec![
            "$__timeFilter(submission_timestamp)".into(),
            format!("normalized_channel = '{channel}'"),
        ],
        group_by: Some("1, 2".into()),
        order_by: Some("time asc, label asc".into()),
        ..Query::default()
    };
    query.add_count_per_day_column("SUM(counter.value)", metric);

    TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1h".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![],
                custom: FieldConfigCustom {
                    axis_label: "count / day".into(),
                    ..FieldConfigCustom::default()
                },
            },
        },
        transformations: vec![Transformation::PartitionByValues {
            fields: vec!["label".into()],
            keep_fields: true,
        }],
        ..TimeSeriesPanel::default()
    }
    .into()
}
