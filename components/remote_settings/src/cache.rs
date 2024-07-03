/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::prelude::*,
};

use parking_lot::Mutex;

use crate::{RemoteSettingsResponse, Result};

/// Interface for caching remote settings data
///
/// This is implemented by the remote settings consumer, sometimes with foreign code via UniFFI.
///
/// This interface has no error handling.  Consumers should report any errors themselves.  If there
/// is an error in `get()` simply return `None` and the remote settings client will re-fetch any
/// data.
pub trait RemoteSettingsCache: Send + Sync {
    /// Update the cached value
    fn store(&self, value: RemoteSettingsResponse);

    /// Get the last value passed to `set`.
    fn get(&self) -> Option<RemoteSettingsResponse>;
}

/// Merge a cached RemoteSettingsResponse and a newly downloaded one to get a merged reesponse
///
/// cached is a previously downloaded remote settings response (possibly run through merge_cache_and_response).
/// new is a newly downloaded remote settings response (with `_expected` set to the last_modified
/// time of the cached response).
///
/// This will merge the records from both responses, handle deletions/tombstones, and return a
/// response that has:
///   - The newest `last_modified_date`
///   - A record list containing the newest version of all live records.  Deleted records will not
///     be present in this list.
///
/// If everything is working properly, the returned value will exactly match what the server would
/// have returned if there was no `_expected` param.
pub fn merge_cache_and_response(
    cached: RemoteSettingsResponse,
    new: RemoteSettingsResponse,
) -> RemoteSettingsResponse {
    let new_record_ids = new
        .records
        .iter()
        .map(|r| r.id.as_str())
        .collect::<HashSet<&str>>();
    // Start with any cached records that don't appear in new.
    let mut records = cached
        .records
        .into_iter()
        .filter(|r| !new_record_ids.contains(r.id.as_str()))
        // deleted should always be false, check it just in case
        .filter(|r| !r.deleted)
        .collect::<Vec<_>>();
    // Add all (non-deleted) records from new
    records.extend(new.records.into_iter().filter(|r| !r.deleted));

    RemoteSettingsResponse {
        last_modified: new.last_modified,
        records,
    }
}

/// Implements RemoteSettingsCache by serializing data and writing it to a file
pub struct RemoteSettingsCacheFile {
    file: Mutex<File>,
}

impl RemoteSettingsCacheFile {
    pub fn new(path: String) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn try_store(&self, value: RemoteSettingsResponse) -> Result<()> {
        let mut file = self.file.lock();
        file.rewind()?;
        serde_json::to_writer(&mut *file, &value)?;
        Ok(())
    }

    fn try_get(&self) -> Result<Option<RemoteSettingsResponse>> {
        let mut file = self.file.lock();
        if file.metadata()?.len() == 0 {
            return Ok(None);
        }
        file.rewind()?;
        Ok(serde_json::from_reader(&mut *file)?)
    }
}

impl RemoteSettingsCache for RemoteSettingsCacheFile {
    fn store(&self, value: RemoteSettingsResponse) {
        if let Err(e) = self.try_store(value) {
            log::warn!("Error writing remote settings cache: {e}");
        }
    }

    fn get(&self) -> Option<RemoteSettingsResponse> {
        match self.try_get() {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Error reading remote settings cache: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{RemoteSettingsRecord, RsJsonObject};

    // Quick way to generate the fields data for our mock records
    fn fields(data: &str) -> RsJsonObject {
        let mut map = serde_json::Map::new();
        map.insert("data".into(), data.into());
        map
    }

    #[test]
    fn test_combine_cache_and_response() {
        let cached_response = RemoteSettingsResponse {
            last_modified: 1000,
            records: vec![
                RemoteSettingsRecord {
                    id: "a".into(),
                    last_modified: 100,
                    deleted: false,
                    attachment: None,
                    fields: fields("a"),
                },
                RemoteSettingsRecord {
                    id: "b".into(),
                    last_modified: 200,
                    deleted: false,
                    attachment: None,
                    fields: fields("b"),
                },
                RemoteSettingsRecord {
                    id: "c".into(),
                    last_modified: 300,
                    deleted: false,
                    attachment: None,
                    fields: fields("c"),
                },
            ],
        };
        let new_response = RemoteSettingsResponse {
            last_modified: 2000,
            records: vec![
                // d is new
                RemoteSettingsRecord {
                    id: "d".into(),
                    last_modified: 1300,
                    deleted: false,
                    attachment: None,
                    fields: fields("d"),
                },
                // b was deleted
                RemoteSettingsRecord {
                    id: "b".into(),
                    last_modified: 1200,
                    deleted: true,
                    attachment: None,
                    fields: RsJsonObject::new(),
                },
                // a was updated
                RemoteSettingsRecord {
                    id: "a".into(),
                    last_modified: 1100,
                    deleted: false,
                    attachment: None,
                    fields: fields("a-with-new-data"),
                },
                // c was not modified, so it's not present in the new response
            ],
        };
        let mut merged = merge_cache_and_response(cached_response, new_response);
        // Sort the records to make the assertion easier
        merged.records.sort_by_key(|r| r.id.clone());
        assert_eq!(
            merged,
            RemoteSettingsResponse {
                last_modified: 2000,
                records: vec![
                    // a was updated
                    RemoteSettingsRecord {
                        id: "a".into(),
                        last_modified: 1100,
                        deleted: false,
                        attachment: None,
                        fields: fields("a-with-new-data"),
                    },
                    RemoteSettingsRecord {
                        id: "c".into(),
                        last_modified: 300,
                        deleted: false,
                        attachment: None,
                        fields: fields("c"),
                    },
                    RemoteSettingsRecord {
                        id: "d".into(),
                        last_modified: 1300,
                        deleted: false,
                        attachment: None,
                        fields: fields("d")
                    },
                ],
            }
        );
    }

    #[test]
    fn test_file_cache() {
        let tempfile = tempfile::NamedTempFile::new().unwrap();
        let path = tempfile.path().to_str().unwrap().to_string();

        // Opening a new file
        let cache = RemoteSettingsCacheFile::new(path.clone()).unwrap();
        assert_eq!(cache.get(), None);
        cache.store(RemoteSettingsResponse {
            last_modified: 1000,
            records: vec![],
        });
        assert_eq!(
            cache.get(),
            Some(RemoteSettingsResponse {
                last_modified: 1000,
                records: vec![],
            })
        );
        drop(cache);

        // Test opening an existing file
        let cache = RemoteSettingsCacheFile::new(path.clone()).unwrap();
        assert_eq!(
            cache.get(),
            Some(RemoteSettingsResponse {
                last_modified: 1000,
                records: vec![],
            })
        );
    }
}
