/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::path::Path;

use error::*;
use ::db::PlacesDb;

// A Places "Connection"
pub struct Connection {
    db: PlacesDb,
}

impl Connection {

    pub fn new(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        let db = PlacesDb::open(path, encryption_key)?;
        Ok(Self { db })
    }

    pub fn new_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        let db = PlacesDb::open_in_memory(encryption_key)?;
        Ok(Self { db })
    }

    pub fn get_db(&self) -> &PlacesDb {
        return &self.db;
    }
}
