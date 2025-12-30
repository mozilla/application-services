/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, DistributionMetricKind, LabeledDistributionMetric, TeamConfig},
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
    metric: &LabeledDistributionMetric,
) -> Result<()> {
    builder.add_panel_title(metric.display_name);
    for app in metric.applications.iter().cloned() {
        builder.add_panel_third(count_panel(app, metric, 5));
        builder.add_panel_third(count_panel(app, metric, 50));
        builder.add_panel_third(count_panel(app, metric, 95));
    }
    Ok(())
}

fn count_panel(
    application: Application,
    metric: &LabeledDistributionMetric,
    percentile: u32,
) -> Panel {
    let LabeledDistributionMetric {
        kind,
        ping,
        category,
        metric,
        value_divisor,
        value_filter,
        axis_label,
        ..
    } = *metric;
    let dataset = application.bigquery_dataset();
    let metric_table = match kind {
        DistributionMetricKind::Memory => "labeled_memory_distribution",
        DistributionMetricKind::Timing => "labeled_timing_distribution",
        DistributionMetricKind::Custom => "labeled_custom_distribution",
    };

    // Group metrics and calculate quantiles.
    // q is a 20-quantile array (0%, 5%, ..., 95%, 100%)
    let mut subquery = Query {
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "CONCAT(metric.key, ' ', normalized_channel) as group_name".into(),
            "APPROX_QUANTILES(CAST(values.key AS INT64), 20) as q".into(),
        ],
        from: format!("`mozdata.{dataset}.{ping}`"),
        joins: vec![
            format!("CROSS JOIN UNNEST(metrics.{metric_table}.{category}_{metric}) as metric"),
            "CROSS JOIN UNNEST(metric.value.values) as values".into(),
            // Cross join with an array with length=values.value to make the APPROX_QUANTILES statement above work.
            // Histogram metrics are stored in bigquery as a struct of key/value pairs.
            // The key is the measurement value, while the value is the count.
            // APPROX_QUANTILES expects to count single values,
            // so use this CROSS JOIN to repeat each key `value` times.
            "CROSS JOIN UNNEST(GENERATE_ARRAY(1, values.value)) AS repeat_number".into(),
        ],
        where_: vec![
            "$__timeFilter(submission_timestamp)".into(),
            "(normalized_channel = 'nightly' OR normalized_channel = 'beta' OR normalized_channel = 'release')".into(),
        ],
        group_by: Some("1, 2".into()),
        ..Query::default()
    };

    if let Some(amount) = value_filter {
        subquery
            .where_
            .push(format!("CAST(values.key AS INT64) >= {amount}"));
    }
    let mut query = Query {
        select: vec!["time".into(), "group_name".into()],
        from: subquery.as_subquery(),
        order_by: Some("time desc, group_name asc".into()),
        ..Query::default()
    };

    let quantile_index = percentile / 5;
    match value_divisor {
        None => {
            query
                .select
                .extend([format!("q[OFFSET({quantile_index})] as amount")]);
        }
        Some(amount) => {
            query
                .select
                .extend([format!("q[OFFSET({quantile_index})] / {amount} as amount")]);
        }
    }

    TimeSeriesPanel {
        title: format!("{application} ({percentile}th percentile)"),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links: vec![],
                custom: FieldConfigCustom {
                    axis_label: axis_label.into(),
                    ..FieldConfigCustom::default()
                },
            },
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["group_name".into()],
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
