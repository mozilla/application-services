/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// We don't want 'db.rs' as a sub-module. We could move the contents here? Or something else?
pub mod db;
pub use crate::db::db::PlacesDb;

mod schema;
