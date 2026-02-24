/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::path::{Path, PathBuf};
use std::time::Duration;

use context_id::{ContextIDComponent, ContextIdCallback};
use uuid::Uuid;

use crate::error::BuildError;
use crate::http_cache::{ByteSize, HttpCache};
use crate::mars::MARSClient;
use crate::telemetry::Telemetry;

use super::config::Environment;
use super::AdsClient;

const DEFAULT_TTL_SECONDS: u64 = 300;
const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;
const DATA_DIR_NAME: &str = "ads-client";

pub struct AdsClientBuilder<T: Telemetry> {
    data_dir: Option<String>,
    environment: Environment,
    cache_ttl_seconds: Option<u64>,
    cache_max_size_mib: Option<u64>,
    legacy_cache_db_path: Option<String>,
    telemetry: T,
}

impl<T> AdsClientBuilder<T>
where
    T: Clone + Telemetry,
{
    pub fn new(telemetry: T) -> Self {
        Self {
            data_dir: None,
            environment: Environment::default(),
            cache_ttl_seconds: None,
            cache_max_size_mib: None,
            legacy_cache_db_path: None,
            telemetry,
        }
    }

    pub fn data_dir(mut self, dir: String) -> Self {
        self.data_dir = Some(dir);
        self
    }

    pub fn environment(mut self, env: Environment) -> Self {
        self.environment = env;
        self
    }

    pub fn cache_ttl_seconds(mut self, ttl: u64) -> Self {
        self.cache_ttl_seconds = Some(ttl);
        self
    }

    pub fn cache_max_size_mib(mut self, size: u64) -> Self {
        self.cache_max_size_mib = Some(size);
        self
    }

    pub fn legacy_cache_db_path(mut self, path: String) -> Self {
        self.legacy_cache_db_path = Some(path);
        self
    }

    pub fn build(self) -> Result<AdsClient<T>, BuildError> {
        let data_dir = resolve_data_dir(
            self.data_dir.as_deref(),
            self.legacy_cache_db_path.as_deref(),
        )
        .ok_or(BuildError::NoDataDir)?;

        std::fs::create_dir_all(&data_dir).map_err(|e| BuildError::CreateDataDir {
            path: data_dir.display().to_string(),
            reason: e.to_string(),
        })?;

        let data_dir_str = data_dir.to_string_lossy().to_string();

        // Read persisted context_id or generate a fresh one
        let (context_id, context_id_ts, callback): (String, i64, Box<dyn ContextIdCallback>) =
            match read_persisted_context_id(&data_dir_str) {
                Some((id, ts)) => (id, ts, Box::new(FileContextIdCallback::new(&data_dir_str))),
                None => (
                    Uuid::new_v4().to_string(),
                    0,
                    Box::new(FileContextIdCallback::new(&data_dir_str)),
                ),
            };

        let context_id_component =
            ContextIDComponent::new(&context_id, context_id_ts, cfg!(test), callback);

        // Build cache at {data_dir}/cache.db
        let cache_db_path = data_dir.join("cache.db");
        let ttl = Duration::from_secs(self.cache_ttl_seconds.unwrap_or(DEFAULT_TTL_SECONDS));
        let max_size = ByteSize::mib(
            self.cache_max_size_mib
                .unwrap_or(DEFAULT_MAX_CACHE_SIZE_MIB),
        );

        let http_cache = match HttpCache::builder(cache_db_path)
            .default_ttl(ttl)
            .max_size(max_size)
            .build()
        {
            Ok(cache) => Some(cache),
            Err(e) => {
                self.telemetry.record(&e);
                None
            }
        };

        let client = MARSClient::new(self.environment, http_cache, self.telemetry.clone());

        Ok(AdsClient::new(client, context_id_component, self.telemetry))
    }
}

/// Resolves the data directory following a fallback chain:
/// 1. Explicit `data_dir` if provided
/// 2. Parent directory of `legacy_cache_db_path` if provided
/// 3. `$HOME` env var
/// 4. `$APPDATA` or `$LOCALAPPDATA` (Windows)
///
/// Always appends an `ads-client/` subdirectory for isolation.
fn resolve_data_dir(
    explicit_data_dir: Option<&str>,
    legacy_cache_db_path: Option<&str>,
) -> Option<PathBuf> {
    // 1. Explicit data_dir
    if let Some(dir) = explicit_data_dir {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return Some(p.join(DATA_DIR_NAME));
        }
    }

    // 2. Parent of legacy cache db_path
    if let Some(db_path) = legacy_cache_db_path {
        let p = Path::new(db_path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                return Some(parent.join(DATA_DIR_NAME));
            }
        }
    }

    // 3. $HOME
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home);
        if !p.as_os_str().is_empty() {
            return Some(p.join(DATA_DIR_NAME));
        }
    }

    // 4. $APPDATA / $LOCALAPPDATA (Windows)
    for var in &["APPDATA", "LOCALAPPDATA"] {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(&val);
            if !p.as_os_str().is_empty() {
                return Some(p.join(DATA_DIR_NAME));
            }
        }
    }

    None
}

/// A file-based implementation of [ContextIdCallback] that persists
/// the context_id and creation timestamp to `{data_dir}/context_id`.
struct FileContextIdCallback {
    file_path: PathBuf,
}

impl FileContextIdCallback {
    fn new(data_dir: &str) -> Self {
        Self {
            file_path: PathBuf::from(data_dir).join("context_id"),
        }
    }
}

impl ContextIdCallback for FileContextIdCallback {
    fn persist(&self, context_id: String, creation_date: i64) {
        let content = format!("{}\n{}", context_id, creation_date);
        if let Err(e) = std::fs::write(&self.file_path, content) {
            eprintln!(
                "Failed to persist context_id to {}: {}",
                self.file_path.display(),
                e
            );
        }
    }

    fn rotated(&self, _old_context_id: String) {
        // No-op: the subsequent persist() call updates the file.
    }
}

/// Reads a previously persisted context_id and creation timestamp from file.
/// Returns `None` if the file is missing, corrupt, or contains an invalid UUID.
fn read_persisted_context_id(data_dir: &str) -> Option<(String, i64)> {
    let file_path = PathBuf::from(data_dir).join("context_id");
    let content = std::fs::read_to_string(&file_path).ok()?;
    let mut lines = content.lines();
    let context_id = lines.next()?.trim().to_string();
    let timestamp_str = lines.next()?.trim();
    let timestamp = timestamp_str.parse::<i64>().ok()?;

    // Validate UUID format
    if Uuid::parse_str(&context_id).is_err() {
        return None;
    }

    Some((context_id, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_data_dir_explicit() {
        let result = resolve_data_dir(Some("/tmp/myapp"), None);
        assert_eq!(result, Some(PathBuf::from("/tmp/myapp/ads-client")));
    }

    #[test]
    fn test_resolve_data_dir_from_legacy_db_path() {
        let result = resolve_data_dir(None, Some("/data/app/cache.db"));
        assert_eq!(result, Some(PathBuf::from("/data/app/ads-client")));
    }

    #[test]
    fn test_resolve_data_dir_explicit_takes_priority() {
        let result = resolve_data_dir(Some("/explicit"), Some("/legacy/cache.db"));
        assert_eq!(result, Some(PathBuf::from("/explicit/ads-client")));
    }

    #[test]
    fn test_resolve_data_dir_empty_explicit_falls_through() {
        let result = resolve_data_dir(Some(""), Some("/legacy/cache.db"));
        assert_eq!(result, Some(PathBuf::from("/legacy/ads-client")));
    }

    #[test]
    fn test_resolve_data_dir_home_fallback() {
        // $HOME is typically set in test environments
        let result = resolve_data_dir(None, None);
        if std::env::var("HOME").is_ok() {
            assert!(result.is_some());
            let path = result.unwrap();
            assert!(path.ends_with("ads-client"));
        }
    }

    #[test]
    fn test_read_persisted_context_id_valid() {
        let dir = std::env::temp_dir().join("ads-client-test-read-valid");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("context_id");
        let uuid = Uuid::new_v4().to_string();
        let ts = 1745859061i64;
        std::fs::write(&file_path, format!("{}\n{}", uuid, ts)).unwrap();

        let result = read_persisted_context_id(dir.to_str().unwrap());
        assert_eq!(result, Some((uuid, ts)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_persisted_context_id_missing_file() {
        let result = read_persisted_context_id("/nonexistent/path");
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_persisted_context_id_invalid_uuid() {
        let dir = std::env::temp_dir().join("ads-client-test-read-invalid-uuid");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("context_id");
        std::fs::write(&file_path, "not-a-uuid\n12345").unwrap();

        let result = read_persisted_context_id(dir.to_str().unwrap());
        assert_eq!(result, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_persisted_context_id_missing_timestamp() {
        let dir = std::env::temp_dir().join("ads-client-test-read-no-ts");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("context_id");
        let uuid = Uuid::new_v4().to_string();
        std::fs::write(&file_path, uuid).unwrap();

        let result = read_persisted_context_id(dir.to_str().unwrap());
        assert_eq!(result, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_file_context_id_callback_roundtrip() {
        let dir = std::env::temp_dir().join("ads-client-test-callback-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_str().unwrap();

        let callback = FileContextIdCallback::new(dir_str);
        let uuid = Uuid::new_v4().to_string();
        let ts = 1745859061i64;
        callback.persist(uuid.clone(), ts);

        let result = read_persisted_context_id(dir_str);
        assert_eq!(result, Some((uuid, ts)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
