/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::path::Path;

use error::*;
use ::db::PlacesDb;
use url::percent_encoding;

// A Places "Connection"
pub struct Connection {
    db: PlacesDb,
}

impl Connection {

    pub fn new(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        let db = PlacesDb::open(path, encryption_key)?;
        Ok(Self { db })
    }

    pub fn new_in_memory(mem_db_name: &str, encryption_key: Option<&str>) -> Result<Self> {
        let name = format!("file:{}?mode=memory&cache=shared",
            percent_encoding::percent_encode(mem_db_name.as_bytes(),
                                             percent_encoding::DEFAULT_ENCODE_SET));

        let db = PlacesDb::open(&name, encryption_key)?;
        Ok(Self { db })
    }

    pub fn get_db(&self) -> &PlacesDb {
        return &self.db;
    }
}
