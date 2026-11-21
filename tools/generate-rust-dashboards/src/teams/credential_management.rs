/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;

use crate::{
    config::*,
    metrics,
    metrics::sync::SyncEngine,
    schema::{TeamDashboards, TeamDashboardsBuilder},
};

pub fn dashboards() -> Result<TeamDashboards> {
    let config = TeamConfig {
        team_name: "Credential Management",
    };
    let mut builder = TeamDashboardsBuilder::new(&config);

    metrics::sync::build_dashboards(
        &config,
        &mut builder,
        &[SyncEngine::Logins, SyncEngine::RustLogins],
    )?;

    metrics::rust_component_errors::build_dashboards(&config, &mut builder, &["logins"])?;

    Ok(builder.finish())
}
