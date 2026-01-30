/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::config::*;

pub fn all_dashboards() -> Vec<TeamConfig> {
    vec![
        TeamConfig {
            team_name: "SYNC",
            components: vec![
                Component::Autofill,
                Component::Fxa,
                Component::Logins,
                Component::Places,
                Component::RemoteSettings,
                Component::Tabs,
            ],
            component_errors: true,
            // Metrics aren't supported yet.  Keep the following fields false/empty for now.
            sync_metrics: false,
            main_dashboard_metrics: vec![],
            extra_dashboards: vec![],
        },
        TeamConfig {
            team_name: "DISCO",
            components: vec![Component::Suggest],
            component_errors: true,
            sync_metrics: false,
            main_dashboard_metrics: vec![],
            extra_dashboards: vec![],
        },
        TeamConfig {
            team_name: "Credential Management",
            components: vec![Component::Logins],
            component_errors: true,
            sync_metrics: false,
            main_dashboard_metrics: vec![],
            extra_dashboards: vec![],
        },
    ]
}
