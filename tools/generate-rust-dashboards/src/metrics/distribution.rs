/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, Unit},
    schema::{
        DashboardBuilder, DataLink, Datasource, FieldConfig, FieldConfigCustom,
        FieldConfigDefaults, GridPos, Panel, ScaleDistribution, Target, TimeSeriesPanel,
        Transformation,
    },
    sql::Query,
    util::UrlBuilder,
    Result,
};

/// Glean timing/memory distribution
///
/// This will create time-series panels for the 5th, 50th and 95th percentile.
pub struct DistributionMetric {
    /// Name to display on the dashboard
    pub display_name: &'static str,
    /// Label describing what we're measure
    pub axis_label: &'static str,
    /// Unit label
    pub unit: Option<Unit>,
    /// Name of the ping ("metrics" by default)
    pub ping: &'static str,
    /// Category name (top-level key in metrics.yaml)
    pub category: &'static str,
    /// Metric name (key for the metric)
    pub metric: &'static str,
    // Which applications report this metric
    pub applications: Vec<Application>,
    // Divide each value by this amount
    //
    // Note:
    // * Timing distributions are always stored in nanoseconds, regardless of the unit listed in
    //   `metrics.yaml`
    // * Memory distributions are always stored in bytes, regardless of the unit listed in
    //   `metrics.yaml`
    pub value_divisor: Option<u64>,
    // Link to an extra dashboard, the inner value should be the `uid` field
    // from one of your team's extra dashboards.
    pub link_to: Option<String>,
}

impl DistributionMetric {
    pub fn build_dashboard(self, builder: &mut DashboardBuilder) -> Result<()> {
        builder.add_panel_title(self.display_name);
        for app in self.applications.iter().cloned() {
            builder.add_panel_third(count_panel(app, &self, "q05", "5th percentile"));
            builder.add_panel_third(count_panel(app, &self, "q50", "50th percentile"));
            builder.add_panel_third(count_panel(app, &self, "q95", "95th percentile"));
        }
        Ok(())
    }
}

fn count_panel(
    application: Application,
    metric: &DistributionMetric,
    quantile: &str,
    quantile_label: &str,
) -> Panel {
    let DistributionMetric {
        ping,
        category,
        metric,
        value_divisor,
        axis_label,
        ref link_to,
        unit,
        ..
    } = *metric;

    let query = Query {
        select: vec![
            "TIMESTAMP(submission_date) as time".into(),
            "channel".into(),
            match value_divisor {
                None => format!("{quantile} as amount"),
                Some(amount) => format!("{quantile} / {amount} as amount"),
            },
        ],
        from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
        order_by: Some("submission_date asc, channel asc".into()),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            "channel IS NOT NULL".into(),
        ],
        ..Query::default()
    };

    let mut links = vec![];
    if let Some(link_to) = link_to {
        links.push(DataLink {
            url: UrlBuilder::new_dashboard(link_to)
                .with_time_range_param()
                .build(),
            target_blank: true,
            one_click: true,
            title: "Details".into(),
        });
    }

    TimeSeriesPanel {
        title: format!("{application} ({quantile_label})"),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
        targets: vec![Target::table(query.sql())],
        field_config: FieldConfig {
            defaults: FieldConfigDefaults {
                links,
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
            ..FieldConfig::default()
        },
        transformations: vec![
            Transformation::PartitionByValues {
                fields: vec!["channel".into()],
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
