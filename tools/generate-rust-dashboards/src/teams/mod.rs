/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Team dashboard generation code
//!
//! To start a new dashboard:
//! * Create a new module for your team
//! * Add a `dashboards()` function, using the existing functions as a template
//! * Add your `dashboards()` function to the all_dashboards() function

mod credential_management;
mod disco;
mod sync;

use anyhow::Result;

use crate::schema::TeamDashboards;

pub fn all_dashboards() -> Result<Vec<TeamDashboards>> {
    Ok(vec![
        sync::dashboards()?,
        disco::dashboards()?,
        credential_management::dashboards()?,
    ])
}
