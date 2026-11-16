/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;

use crate::{
    config::*,
    metrics,
    schema::{TeamDashboards, TeamDashboardsBuilder},
};

pub fn dashboards() -> Result<TeamDashboards> {
    let config = TeamConfig { team_name: "DISCO" };

    let mut builder = TeamDashboardsBuilder::new(&config);

    metrics::rust_component_errors::build_dashboards(&config, &mut builder, &["suggest"])?;

    metrics::LabeledDistributionMetric {
        display_name: "Suggest ingest time",
        ping: "metrics",
        category: "suggest",
        metric: "ingest_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Desktop],
    }
    .build_dashboard(&mut builder.main)?;

    metrics::LabeledDistributionMetric {
        display_name: "Suggest ingest download time",
        ping: "metrics",
        category: "suggest",
        metric: "ingest_download_time",
        axis_label: "time",
        unit: Some(Unit::Milliseconds),
        value_divisor: Some(1_000_000),
        applications: vec![Application::Desktop],
    }
    .build_dashboard(&mut builder.main)?;

    metrics::LabeledDistributionMetric {
        display_name: "Suggest query time",
        ping: "metrics",
        category: "suggest",
        metric: "query_time",
        axis_label: "time",
        unit: Some(Unit::Microseconds),
        value_divisor: Some(1_000),
        applications: vec![Application::Desktop],
    }
    .build_dashboard(&mut builder.main)?;

    Ok(builder.finish())
}
