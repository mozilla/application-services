/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{config::TeamConfig, schema::DashboardBuilder};

pub fn start_dashboard(config: &TeamConfig) -> DashboardBuilder {
    DashboardBuilder::new(
        config.team_name.to_string(),
        format!("{}-main", config.team_slug()),
    )
}
