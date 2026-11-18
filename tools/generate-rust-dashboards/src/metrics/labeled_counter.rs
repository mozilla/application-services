/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel},
    schema::{DashboardBuilder, Panel, TimeSeriesPanel},
    sql::{Query, Union},
    Result,
};

/// Glean labeled counter
///
/// This will create time-series panels for the counter, partitioned by the label
pub struct LabeledCounterMetric {
    /// Name to display on the dashboard
    pub display_name: &'static str,
    /// Name of the ping ("metrics" by default)
    pub ping: &'static str,
    /// Category name (top-level key in metrics.yaml)
    pub category: &'static str,
    /// Metric name (key for the metric)
    pub metric: &'static str,
    // Which applications report this metric
    pub applications: Vec<Application>,
    pub options: LabeledCounterOptions,
}

#[derive(Default)]
pub struct LabeledCounterOptions {
    pub unique_user_counts: bool,
    // List of all possible labels.  Listing these can result in a nicer color scheme, especially
    // when `unique_user_counts` is set.
    pub labels: Option<Vec<String>>,
}

impl LabeledCounterMetric {
    pub fn build_dashboard(self, builder: &mut DashboardBuilder) -> Result<()> {
        builder.add_panel_title(self.display_name);
        for app in self.applications.iter().cloned() {
            builder.add_panel_third(count_panel(app, ReleaseChannel::Nightly, &self));
            builder.add_panel_third(count_panel(app, ReleaseChannel::Beta, &self));
            builder.add_panel_third(count_panel(app, ReleaseChannel::Release, &self));
        }
        Ok(())
    }
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

    TimeSeriesPanel::new_grouped(
        application.display_name(channel),
        union.sql(),
        "count",
        "label",
    )
    .with_axis_label("count / day")
    .into()
}
