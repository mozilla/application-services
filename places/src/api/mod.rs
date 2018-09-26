/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod history;
pub mod autocomplete;
pub mod matcher;
use db::PlacesDb;
use error::{Result};
use observation::{VisitObservation};
use storage;

pub fn apply_observation(conn: &mut PlacesDb, visit_obs: VisitObservation) -> Result<()> {
    storage::apply_observation(conn, visit_obs)
}
