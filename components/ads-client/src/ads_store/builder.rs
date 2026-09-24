/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::connection_initializer::AdsStoreConnectionInitializer;
use crate::ads_store::store::AdsStoreHolder;
use crate::ads_store::AdsStore;
use crate::common::bytesize::ByteSize;
use crate::telemetry::Telemetry;
use rusqlite::Connection;
use sql_support::open_database;
use std::path::PathBuf;

const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const MIN_STORE_SIZE: ByteSize = ByteSize::kib(1);
const MAX_STORE_SIZE: ByteSize = ByteSize::mib(100);

#[derive(Debug, thiserror::Error)]
pub enum AdsStoreBuilderError {
    #[error("Database error: {0}")]
    Database(#[from] open_database::Error),
    #[error("Database path cannot be empty")]
    EmptyDbPath,
    #[error(
        "Maximum store size must be between {min_size} and {max_size}, got {size_bytes} bytes"
    )]
    InvalidMaxSize {
        max_size: String,
        min_size: String,
        size_bytes: u64,
    },
}

pub struct AdsStoreBuilder {
    db_path: PathBuf,
    max_size: Option<ByteSize>,

    // Flag for whether this db is built in-memory
    // This is only set by an error in `open_connection`, not manually.
    is_memory: bool,
}

impl AdsStoreBuilder {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            max_size: None,
            is_memory: false,
        }
    }

    pub fn max_size(mut self, max_size: ByteSize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    fn open_connection(
        &mut self,
        telemetry: impl Telemetry,
    ) -> Result<Connection, AdsStoreBuilderError> {
        let initializer = AdsStoreConnectionInitializer {};
        if !cfg!(test) {
            match open_database::open_database(&self.db_path, &initializer) {
                Ok(conn) => return Ok(conn),
                Err(e) => telemetry.record(&AdsStoreBuilderError::from(e)),
            }
        }

        self.is_memory = true;
        Ok(open_database::open_memory_database(&initializer)?)
    }

    fn validate(&self) -> Result<(), AdsStoreBuilderError> {
        if self.db_path.to_string_lossy().trim().is_empty() {
            return Err(AdsStoreBuilderError::EmptyDbPath);
        }

        if let Some(max_size) = self.max_size {
            if max_size < MIN_STORE_SIZE || max_size > MAX_STORE_SIZE {
                return Err(AdsStoreBuilderError::InvalidMaxSize {
                    size_bytes: max_size.as_u64(),
                    min_size: MIN_STORE_SIZE.to_string(),
                    max_size: MAX_STORE_SIZE.to_string(),
                });
            }
        }

        Ok(())
    }

    pub fn build(&mut self, telemetry: impl Telemetry) -> Result<AdsStore, AdsStoreBuilderError> {
        self.validate()?;

        let conn = self.open_connection(telemetry)?;
        let holder = AdsStoreHolder::new(conn);
        let max_size = self.max_size.unwrap_or(DEFAULT_MAX_SIZE);
        Ok(AdsStore {
            max_size,
            holder,
            is_memory: self.is_memory,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::telemetry::MozAdsTelemetryWrapper;

    fn make_test_builder(path: &str) -> AdsStoreBuilder {
        AdsStoreBuilder::new(path)
    }

    #[test]
    fn test_store_builder_with_defaults() {
        let mut builder = make_test_builder("test.db");
        assert_eq!(builder.db_path, PathBuf::from("test.db"));
        assert_eq!(builder.max_size, None);
        assert!(builder.build(MozAdsTelemetryWrapper::noop()).is_ok());
    }

    #[test]
    fn test_cache_builder_valid_custom() {
        let mut builder = make_test_builder("custom.db").max_size(ByteSize::b(1024));

        assert_eq!(builder.db_path, PathBuf::from("custom.db"));
        assert_eq!(builder.max_size, Some(ByteSize::b(1024)));
        assert!(builder.build(MozAdsTelemetryWrapper::noop()).is_ok());
    }

    #[test]
    fn test_validation_empty_db_path() {
        let result = make_test_builder("   ").build(MozAdsTelemetryWrapper::noop());
        assert!(matches!(result, Err(AdsStoreBuilderError::EmptyDbPath)));
    }

    #[test]
    fn test_validation_max_size_too_small() {
        let result = make_test_builder("test.db")
            .max_size(ByteSize::b(512))
            .build(MozAdsTelemetryWrapper::noop());
        assert!(matches!(
            result,
            Err(AdsStoreBuilderError::InvalidMaxSize {
                size_bytes: 512,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_too_large() {
        let result = make_test_builder("test.db")
            .max_size(ByteSize::b(2 * 1024 * 1024 * 1024))
            .build(MozAdsTelemetryWrapper::noop());
        assert!(matches!(
            result,
            Err(AdsStoreBuilderError::InvalidMaxSize {
                size_bytes: 2147483648,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_boundaries() {
        let mut builder_min = make_test_builder("test.db").max_size(MIN_STORE_SIZE);
        assert!(builder_min.build(MozAdsTelemetryWrapper::noop()).is_ok());

        let mut builder_max = make_test_builder("test.db").max_size(MAX_STORE_SIZE);
        assert!(builder_max.build(MozAdsTelemetryWrapper::noop()).is_ok());
    }
}
