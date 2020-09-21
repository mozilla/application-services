/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::AutofillDb;
use crate::error::*;
use std::path::Path;

#[allow(dead_code)]
pub struct Store {
    db: AutofillDb,
}

impl Store {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            db: AutofillDb::new(db_path)?,
        })
    }

    /// Creates a store backed by an in-memory database.
    #[cfg(test)]
    pub fn new_memory(db_path: &str) -> Result<Self> {
        Ok(Self {
            db: AutofillDb::new_memory(db_path)?,
        })
    }
}
