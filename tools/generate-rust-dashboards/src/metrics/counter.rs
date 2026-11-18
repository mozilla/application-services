/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel},
    schema::{DashboardBuilder, Panel, TimeSeriesPanel},
    sql::Query,
    Result,
};

/// Glean counter
///
/// This will create time-series panels for the counter
pub struct CounterMetric {
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
    pub options: CounterOptions,
}

#[derive(Default)]
pub struct CounterOptions {
    pub unique_user_counts: bool,
}

impl CounterMetric {
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

fn count_panel(application: Application, channel: ReleaseChannel, metric: &CounterMetric) -> Panel {
    // Note: some of this code is untested since we've never had any counter metrics used in
    // practice

    let CounterMetric {
        ping,
        category,
        metric,
        options,
        ..
    } = metric;

    let mut select = vec!["TIMESTAMP(submission_date) as time".into(), "count".into()];
    if options.unique_user_counts {
        select.push("client_count".into());
    }

    let query = Query {
        select,
        from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            format!("application = `{}`", application.slug()),
            format!("channel = '{channel}'"),
        ],
        order_by: Some("submission_date asc".into()),
        ..Query::default()
    };

    TimeSeriesPanel::new(application.display_name(channel), query.sql())
        .with_axis_label("count / day")
        .into()
}
