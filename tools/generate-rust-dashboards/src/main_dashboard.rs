/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    config::{Component, TeamConfig},
    schema::{CustomVariable, CustomVariableSelection, DashboardBuilder},
    util::Join,
};

pub fn start_dashboard(config: &TeamConfig) -> DashboardBuilder {
    let mut builder = DashboardBuilder::new(
        config.team_name.to_string(),
        format!("{}-main", config.team_slug()),
    );

    // Components variable
    builder.add_variable(CustomVariable {
        label: "Components".into(),
        name: "components".into(),
        multi: true,
        query: config.components.iter().map(Component::slug).join(","),
        current: CustomVariableSelection::multi(config.components.iter().map(Component::slug)),
        ..CustomVariable::default()
    });

    builder
}
