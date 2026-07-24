/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::path::PathBuf;

use crate::impression_log::connection_initializer::ImpressionLogConnectionInitializer;
use crate::impression_log::store::ImpressionLogStore;
use crate::impression_log::ImpressionLog;

use rusqlite::Connection;
use sql_support::open_database;

#[derive(Debug, thiserror::Error)]
pub enum ImpressionLogBuilderError {
    #[error("Database path cannot be empty")]
    EmptyDbPath,
    #[error("Database error: {0}")]
    Database(#[from] open_database::Error),
}

pub struct ImpressionLogBuilder {
    db_path: PathBuf,
}

impl ImpressionLogBuilder {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    fn validate(&self) -> Result<(), ImpressionLogBuilderError> {
        if self.db_path.to_string_lossy().trim().is_empty() {
            return Err(ImpressionLogBuilderError::EmptyDbPath);
        }

        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, ImpressionLogBuilderError> {
        let initializer = ImpressionLogConnectionInitializer {};
        let conn = if cfg!(test) {
            open_database::open_memory_database(&initializer)?
        } else {
            open_database::open_database(&self.db_path, &initializer)?
        };
        Ok(conn)
    }

    pub fn build(&self) -> Result<ImpressionLog, ImpressionLogBuilderError> {
        self.validate()?;

        let conn = self.open_connection()?;
        let store = ImpressionLogStore::new(conn);

        Ok(ImpressionLog { store })
    }
}
