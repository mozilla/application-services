/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel, Unit},
    schema::{DashboardBuilder, Panel, TimeSeriesPanel},
    sql::Query,
    Result,
};

/// Glean labeled timing/memory distribution
///
/// This will create time-series panels for the 5th, 50th and 95th percentile.
/// Percentiles will be partitioned by the metric label.
pub struct LabeledDistributionMetric {
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
}

impl LabeledDistributionMetric {
    pub fn build_dashboard(self, builder: &mut DashboardBuilder) -> Result<()> {
        builder.add_panel_title(self.display_name);
        for app in self.applications.iter().cloned() {
            for channel in ReleaseChannel::all() {
                builder.add_panel_third(count_panel(app, channel, &self, "q05", "5th percentile"));
                builder.add_panel_third(count_panel(app, channel, &self, "q50", "50th percentile"));
                builder.add_panel_third(count_panel(app, channel, &self, "q95", "95th percentile"));
            }
        }
        Ok(())
    }
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

    let mut p = TimeSeriesPanel::new_grouped(
        format!("{application} - {channel} ({quantile_label})"),
        query.sql(),
        "amount",
        "label",
    )
    .with_axis_label(axis_label)
    .with_log_distribution(10);

    if let Some(unit) = unit {
        p = p.with_unit(unit);
    }
    p.into()
}
