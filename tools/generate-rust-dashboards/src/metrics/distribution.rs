/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, DistributionMetric, DistributionMetricKind, TeamConfig},
    schema::{
        DashboardBuilder, DataLink, Datasource, FieldConfig, FieldConfigCustom,
        FieldConfigDefaults, GridPos, Panel, Target, TimeSeriesPanel, Transformation,
    },
    sql::Query,
    util::{slug, UrlBuilder},
    Result,
};

pub fn add_to_dashboard(
    builder: &mut DashboardBuilder,
    _config: &TeamConfig,
    metric: &DistributionMetric,
) -> Result<()> {
    builder.add_panel_title(metric.display_name);
    for app in metric.applications.iter().cloned() {
        builder.add_panel_third(count_panel(app, metric, 5));
        builder.add_panel_third(count_panel(app, metric, 50));
        builder.add_panel_third(count_panel(app, metric, 95));
    }
    Ok(())
}

fn count_panel(application: Application, metric: &DistributionMetric, percentile: u32) -> Panel {
    let DistributionMetric {
        kind,
        ping,
        category,
        metric,
        value_divisor,
        value_filter,
        axis_label,
        link_to,
        ..
    } = *metric;
    let dataset = application.bigquery_dataset();
    let metric_table = match kind {
        DistributionMetricKind::Memory => "memory_distribution",
        DistributionMetricKind::Timing => "timing_distribution",
        DistributionMetricKind::Custom => "custom_distribution",
    };

    // Group metrics and calculate quantiles.
    // q is a 20-quantile array (0%, 5%, ..., 95%, 100%)
    let mut subquery = Query {
        select: vec![
            "$__timeGroup(submission_timestamp, $__interval) as time".into(),
            "normalized_channel".into(),
            "APPROX_QUANTILES(CAST(values.key AS INT64), 20) as q".into(),
        ],
        from: format!("`mozdata.{dataset}.{ping}`"),
        joins: vec![
            format!("CROSS JOIN UNNEST(metrics.{metric_table}.{category}_{metric}.values) as values"),
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
        select: vec!["time".into(), "normalized_channel".into()],
        from: subquery.as_subquery(),
        order_by: Some("time desc, normalized_channel asc".into()),
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

    let mut links = vec![];
    if let Some(link_to) = link_to {
        links.push(DataLink {
            url: UrlBuilder::new_dashboard(slug(link_to))
                .with_time_range_param()
                .build(),
            target_blank: true,
            one_click: true,
            title: "Details".into(),
        });
    }

    TimeSeriesPanel {
        title: format!("{application} ({percentile}th percentile)"),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links,
                custom: FieldConfigCustom {
                    axis_label: axis_label.into(),
                    ..FieldConfigCustom::default()
                },
            },
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["normalized_channel".into()],
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
