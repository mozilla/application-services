/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Application, CounterMetric, ReleaseChannel, TeamConfig},
    schema::{
        DashboardBuilder, Datasource, FieldConfig, FieldConfigCustom, FieldConfigDefaults, GridPos,
        Panel, Target, TimeSeriesPanel,
    },
    sql::Query,
    Result,
};

pub fn add_to_dashboard(
    builder: &mut DashboardBuilder,
    _config: &TeamConfig,
    metric: &CounterMetric,
) -> Result<()> {
    builder.add_panel_title(metric.display_name);
    for app in metric.applications.iter().cloned() {
        builder.add_panel_third(count_panel(app, ReleaseChannel::Nightly, metric));
        builder.add_panel_third(count_panel(app, ReleaseChannel::Beta, metric));
        builder.add_panel_third(count_panel(app, ReleaseChannel::Release, metric));
    }
    Ok(())
}

fn count_panel(application: Application, channel: ReleaseChannel, metric: &CounterMetric) -> Panel {
    let CounterMetric {
        ping,
        category,
        metric,
        ..
    } = *metric;

    let query = Query {
        select: vec!["TIMESTAMP(submission_date) as time".into(), "count".into()],
        from: format!("`mozdata.rust_components.{ping}_{category}_{metric}`"),
        where_: vec![
            "$__timeFilter(TIMESTAMP(submission_date))".into(),
            format!("application = `{}`", application.slug()),
            format!("channel = '{channel}'"),
        ],
        order_by: Some("submission_date asc".into()),
        ..Query::default()
    };

    TimeSeriesPanel {
        title: application.display_name(channel),
        grid_pos: GridPos::height(8),
        datasource: Datasource::bigquery(),
        interval: "1d".into(),
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
        transformations: vec![],
        ..TimeSeriesPanel::default()
    }
    .into()
}
