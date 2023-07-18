/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use once_cell::sync::OnceCell;
use remote_settings::{self, Attachment, GetItemsOptions, RemoteSettingsConfig, SortOrder};
use serde::Deserialize;

use crate::{
    db::{ConnectionType, SuggestDb, LAST_FETCH_META_KEY},
    RemoteRecordId, RemoteSuggestion, Result, SuggestApiResult, Suggestion, SuggestionQuery,
};

const RS_COLLECTION: &str = "quicksuggest";

/// The store is the entry point to the Suggest component. It incrementally
/// fetches suggestions from the Remote Settings service, stores them in a local
/// database, and returns them in response to user queries.
///
/// Your application should create a single store, and manage it as a singleton.
/// The store is thread-safe, and supports concurrent fetches and ingests. We
/// expect that your application will call `query` to show suggestions as the
/// user types into the address bar, and periodically call `ingest` in the
/// background to update the database with new suggestions from Remote Settings.
///
/// The store keeps track of the state needed to support incremental ingestion,
/// but doesn't schedule the ingestion work itself, or decide how many
/// suggestions to ingest at once. This is for two reasons:
///
/// 1. The primitives for scheduling background work vary between platforms. You
///    might use an idle timer on Desktop, `WorkManager` on Android, or
///    `BGTaskScheduler` on iOS.
/// 2. Ingestion limits can change, depending on the platform and the needs of
///    the application. A mobile device on a metered connection might want to
///    request a small subset of the Suggest data and fetch the rest later,
///    while a desktop on a fast link might request the entire dataset on first
///    launch.
pub struct SuggestStore {
    path: PathBuf,
    dbs: OnceCell<SuggestStoreDbs>,
    settings_client: remote_settings::Client,
}

/// Limits for an ingestion from Remote Settings.
pub struct IngestLimits {
    /// The maximum number of records to request from Remote Settings.
    /// Each record has about 200 suggestions.
    pub records: Option<u64>,
}

impl SuggestStore {
    /// Creates a Suggest store.
    pub fn new(
        path: &str,
        settings_config: Option<RemoteSettingsConfig>,
    ) -> SuggestApiResult<Self> {
        Ok(Self::new_inner(path, settings_config)?)
    }

    fn new_inner(
        path: impl AsRef<Path>,
        settings_config: Option<RemoteSettingsConfig>,
    ) -> Result<Self> {
        let settings_client = remote_settings::Client::new(settings_config.unwrap_or_else(|| {
            RemoteSettingsConfig {
                server_url: None,
                bucket_name: None,
                collection_name: RS_COLLECTION.into(),
            }
        }))?;
        Ok(Self {
            path: path.as_ref().into(),
            dbs: OnceCell::new(),
            settings_client,
        })
    }

    /// Returns this provider's database connections, initializing them if
    /// they're not already open.
    fn dbs(&self) -> Result<&SuggestStoreDbs> {
        self.dbs
            .get_or_try_init(|| SuggestStoreDbs::open(&self.path))
    }

    /// Queries the database for suggestions that match the `keyword`.
    pub fn query(&self, query: &SuggestionQuery) -> SuggestApiResult<Vec<Suggestion>> {
        if query.keyword.is_empty() {
            return Ok(Vec::new());
        }
        let suggestions = self
            .dbs()?
            .reader
            .read(|dao| dao.fetch_by_keyword(&query.keyword))?;
        Ok(suggestions
            .into_iter()
            .filter(|suggestion| {
                (suggestion.is_sponsored && query.include_sponsored)
                    || (!suggestion.is_sponsored && query.include_non_sponsored)
            })
            .collect())
    }

    /// Interrupts any ongoing queries. This should be called when the
    /// user types a new keyword into the address bar, to ensure that they
    /// see fresh suggestions as they type.
    pub fn interrupt(&self) {
        if let Some(dbs) = self.dbs.get() {
            // Only interrupt if the databases are already open.
            dbs.reader.interrupt_handle.interrupt();
        }
    }

    /// Ingests new suggestions from Remote Settings. `limits` can be used to
    /// constrain the amount of work done.
    pub fn ingest(&self, limits: &IngestLimits) -> SuggestApiResult<()> {
        Ok(self.ingest_inner(limits)?)
    }

    fn ingest_inner(&self, limits: &IngestLimits) -> Result<()> {
        let writer = &self.dbs()?.writer;

        let mut options = GetItemsOptions::new();
        // Remote Settings returns records in descending modification order
        // (newest first), but we want them in ascending order (oldest first),
        // so that we can eventually resume fetching where we left off.
        options.sort("last_modified", SortOrder::Ascending);
        if let Some(last_fetch) = writer.read(|dao| dao.get_meta::<u64>(LAST_FETCH_META_KEY))? {
            // Only fetch changes since our last fetch. If our last fetch was
            // interrupted, we'll pick up where we left off.
            options.gt("last_modified", last_fetch.to_string());
        }
        if let Some(records) = &limits.records {
            options.limit(*records);
        }

        let records = self
            .settings_client
            .get_records_raw_with_options(&options)?
            .json::<SuggestRecordsResponse>()?
            .data;
        for record in &records {
            match record {
                FetchedChange::Record(SuggestRecord::Data {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    let suggestions = self
                        .settings_client
                        .get_attachment(&attachment.location)?
                        .json::<SuggestAttachmentData>()?
                        .0;

                    writer.write(|dao| {
                        // Drop any suggestions that we previously ingested from
                        // this record's attachment. Suggestions don't have a
                        // stable identifier, and determining which suggestions in
                        // the attachment actually changed is more complicated than
                        // dropping and re-ingesting all of them.
                        dao.drop_suggestions(record_id)?;

                        // Ingest (or re-ingest) all suggestions in the attachment.
                        dao.insert_suggestions(record_id, &suggestions)?;

                        // Advance the last fetch time, so that we can resume
                        // fetching after this record if we're interrupted.
                        dao.put_meta(LAST_FETCH_META_KEY, last_modified)?;

                        Ok(())
                    })?;
                }
                FetchedChange::Unknown {
                    id: record_id,
                    last_modified,
                    deleted,
                } if *deleted => {
                    // If the entire record was deleted, drop all its
                    // suggestions and advance the last fetch time.
                    writer.write(|dao| {
                        match record_id.as_icon_id() {
                            Some(icon_id) => dao.drop_icon(icon_id)?,
                            None => dao.drop_suggestions(record_id)?,
                        };
                        dao.put_meta(LAST_FETCH_META_KEY, last_modified)?;
                        Ok(())
                    })?;
                }
                FetchedChange::Record(SuggestRecord::Icon {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    let Some(icon_id) = record_id.as_icon_id() else {
                        continue
                    };
                    let data = self
                        .settings_client
                        .get_attachment(&attachment.location)?
                        .body;
                    writer.write(|dao| {
                        dao.insert_icon(icon_id, &data)?;
                        dao.put_meta(LAST_FETCH_META_KEY, last_modified)?;
                        Ok(())
                    })?;
                }
                _ => continue,
            }
        }

        Ok(())
    }

    pub fn clear(&self) -> SuggestApiResult<()> {
        Ok(self.dbs()?.writer.write(|dao| dao.clear())?)
    }
}

struct SuggestStoreDbs {
    /// A read-write connection used to update the database with new data.
    writer: SuggestDb,
    /// A read-only connection used to query the database.
    reader: SuggestDb,
}

impl SuggestStoreDbs {
    fn open(path: &Path) -> Result<Self> {
        // Order is important here: the writer must be opened first, so that it
        // can set up the database and run any migrations.
        let writer = SuggestDb::open(path, ConnectionType::ReadWrite)?;
        let reader = SuggestDb::open(path, ConnectionType::ReadOnly)?;
        Ok(Self { writer, reader })
    }
}

#[derive(Debug, Deserialize)]
struct SuggestRecordsResponse {
    data: Vec<FetchedChange>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type")]
enum SuggestRecord {
    #[serde(rename = "icon")]
    Icon {
        id: RemoteRecordId,
        last_modified: u64,
        attachment: Attachment,
    },
    #[serde(rename = "data")]
    Data {
        id: RemoteRecordId,
        last_modified: u64,
        attachment: Attachment,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(untagged)]
enum FetchedChange {
    Record(SuggestRecord),
    Unknown {
        id: RemoteRecordId,
        last_modified: u64,
        #[serde(default)]
        deleted: bool,
    },
}

/// Represents either a single value, or a list of values. This is used to
/// deserialize suggestion attachment bodies.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> Deref for OneOrMany<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        match self {
            OneOrMany::One(value) => std::slice::from_ref(value),
            OneOrMany::Many(values) => values,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct SuggestAttachmentData(OneOrMany<RemoteSuggestion>);

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Once;

    use mockito::{mock, Matcher};
    use serde_json::json;

    fn before_each() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            env_logger::init();
        });
    }

    #[test]
    fn is_thread_safe() {
        before_each();

        // Ensure that `SuggestStore` is usable with UniFFI, which requires
        // exposed interfaces to be `Send` and `Sync`.
        fn is_send_sync<T: Send + Sync>() {}
        is_send_sync::<SuggestStore>();
    }

    #[test]
    fn ingest() -> anyhow::Result<()> {
        before_each();

        viaduct_reqwest::use_reqwest_backend();

        let server_info_m = mock("GET", "/")
            .with_body(serde_json::to_vec(&attachment_metadata(&mockito::server_url())).unwrap())
            .with_status(200)
            .with_header("content-type", "application/json")
            .create();

        let records = json!({
            "data": [{
                "id": "1234",
                "type": "data",
                "last_modified": 15,
                "attachment": {
                    "filename": "data-1.json",
                    "mimetype": "application/json",
                    "location": "data-1.json",
                    "hash": "",
                    "size": 0,
                },
            }],
        });
        let records_m = mock("GET", "/v1/buckets/main/collections/quicksuggest/records")
            .match_query(Matcher::Any)
            .with_body(serde_json::to_vec(&records).unwrap())
            .with_status(200)
            .with_header("content-type", "application/json")
            .create();

        let attachment = json!([{
            "id": 0,
            "advertiser": "Los Pollos Hermanos",
            "iab_category": "8 - Food & Drink",
            "keywords": ["lo", "los", "los p", "los pollos", "los pollos h", "los pollos hermanos"],
            "title": "Los Pollos Hermanos - Albuquerque",
            "url": "https://www.lph-nm.biz",
            "icon": "5678",
        }]);
        let attachment_m = mock("GET", "/attachments/data-1.json")
            .with_body(serde_json::to_vec(&attachment).unwrap())
            .with_status(200)
            .with_header("content-type", "application/json")
            .create();

        let settings_config = RemoteSettingsConfig {
            server_url: Some(mockito::server_url()),
            bucket_name: None,
            collection_name: "quicksuggest".into(),
        };

        let store = SuggestStore::new_inner(
            "file:ingest?mode=memory&cache=shared",
            Some(settings_config),
        )?;
        store.ingest(&IngestLimits { records: None })?;

        server_info_m.expect(1).assert();
        records_m.expect(1).assert();
        attachment_m.expect(1).assert();

        assert_eq!(
            Some(15u64),
            store
                .dbs()?
                .reader
                .read(|dao| dao.get_meta(LAST_FETCH_META_KEY))?,
        );

        let suggestions = store.query(&SuggestionQuery {
            keyword: "lo".into(),
            include_sponsored: true,
            ..Default::default()
        })?;
        assert_eq!(
            suggestions,
            &[Suggestion {
                block_id: 0,
                advertiser: "Los Pollos Hermanos".into(),
                iab_category: "8 - Food & Drink".into(),
                is_sponsored: true,
                full_keyword: "los".into(),
                title: "Los Pollos Hermanos - Albuquerque".into(),
                url: "https://www.lph-nm.biz".into(),
                icon: None,
                impression_url: None,
                click_url: None,
            }]
        );

        Ok(())
    }

    fn attachment_metadata(base_url: &str) -> serde_json::Value {
        json!({
            "capabilities": {
                "attachments": {
                    "base_url": format!("{}/attachments/", base_url),
                },
            },
        })
    }
}
