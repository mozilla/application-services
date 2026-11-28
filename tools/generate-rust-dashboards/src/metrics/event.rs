/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, ReleaseChannel},
    schema::{DashboardBuilder, Panel, TimeSeriesPanel},
    sql::{Query, Union},
    Result,
};

/// Track multiple Glean events together
///
/// This will create time-series panels with event counts for each event
pub struct EventsMetric {
    /// Name to display on the dashboard
    pub display_name: &'static str,
    /// Name of the ping ("metrics" by default)
    pub ping: &'static str,
    /// Category name (top-level key in metrics.yaml)
    pub category: &'static str,
    /// Metric name (key for the metric)
    pub metrics: Vec<&'static str>,
    // Which applications report this metric
    pub applications: Vec<Application>,
    pub options: EventsOptions,
}

#[derive(Default)]
pub struct EventsOptions {
    pub unique_user_counts: bool,
}

impl EventsMetric {
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

fn count_panel(application: Application, channel: ReleaseChannel, metric: &EventsMetric) -> Panel {
    let EventsMetric {
        ping,
        category,
        metrics,
        options,
        ..
    } = metric;

    let mut query = Union::default();
    for metric in metrics {
        let base_query = Query {
            from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
            where_: vec![
                "$__timeFilter(TIMESTAMP(submission_date))".into(),
                format!("application = '{}'", application.slug()),
                format!("channel = '{channel}'"),
            ],
            group_by: Some("1, 2".into()),
            ..Query::default()
        };

        query.queries.push(Query {
            select: vec![
                "TIMESTAMP(submission_date) as time".into(),
                format!("'{metric}' as label"),
                "SUM(count) as count".into(),
            ],
            ..base_query.clone()
        });
        if options.unique_user_counts {
            query.queries.push(Query {
                select: vec![
                    "TIMESTAMP(submission_date) as time".into(),
                    format!("'{metric} (daily unique users)' as label"),
                    "SUM(client_count) as count".into(),
                ],
                ..base_query.clone()
            });
        }
    }
    query.order_by = Some("submission_date asc".into());

    let mut p = TimeSeriesPanel::new_grouped(
        application.display_name(channel),
        query.sql(),
        "count",
        "label",
    )
    .with_axis_label("count / day");

    for (i, metric) in metrics.iter().enumerate() {
        p = p.with_label_color(*metric, i, false);
        if options.unique_user_counts {
            p = p.with_label_color(format!("{metric} (daily unique users)"), i, true);
        }
    }

    p.into()
}
