/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::path::{Path, PathBuf};

use once_cell::sync::OnceCell;
use remote_settings::{self, GetItemsOptions, RemoteSettingsConfig, SortOrder};

use crate::{
    db::{ConnectionType, SuggestDb, LAST_INGEST_META_KEY},
    rs::{
        SuggestRecord, SuggestRemoteSettingsClient, TypedSuggestRecord, REMOTE_SETTINGS_COLLECTION,
        SUGGESTIONS_PER_ATTACHMENT,
    },
    Result, SuggestApiResult, Suggestion, SuggestionQuery,
};

/// The store is the entry point to the Suggest component. It incrementally
/// downloads suggestions from the Remote Settings service, stores them in a
/// local database, and returns them in response to user queries.
///
/// Your application should create a single store, and manage it as a singleton.
/// The store is thread-safe, and supports concurrent queries and ingests. We
/// expect that your application will call [`SuggestStore::query()`] to show
/// suggestions as the user types into the address bar, and periodically call
/// [`SuggestStore::ingest()`] in the background to update the database with
/// new suggestions from Remote Settings.
///
/// For responsiveness, we recommend always calling `query()` on a worker
/// thread. When the user types new input into the address bar, call
/// [`SuggestStore::interrupt()`] on the main thread to cancel the query
/// for the old input, and unblock the worker thread for the new query.
///
/// The store keeps track of the state needed to support incremental ingestion,
/// but doesn't schedule the ingestion work itself, or decide how many
/// suggestions to ingest at once. This is for two reasons:
///
/// 1. The primitives for scheduling background work vary between platforms, and
///    aren't available to the lower-level Rust layer. You might use an idle
///    timer on Desktop, `WorkManager` on Android, or `BGTaskScheduler` on iOS.
/// 2. Ingestion constraints can change, depending on the platform and the needs
///    of your application. A mobile device on a metered connection might want
///    to request a small subset of the Suggest data and download the rest
///    later, while a desktop on a fast link might download the entire dataset
///    on the first launch.
pub struct SuggestStore {
    inner: SuggestStoreInner<remote_settings::Client>,
}

impl SuggestStore {
    /// Creates a Suggest store.
    pub fn new(
        path: &str,
        settings_config: Option<RemoteSettingsConfig>,
    ) -> SuggestApiResult<Self> {
        let settings_client = || -> Result<_> {
            Ok(remote_settings::Client::new(
                settings_config.unwrap_or_else(|| RemoteSettingsConfig {
                    server_url: None,
                    bucket_name: None,
                    collection_name: REMOTE_SETTINGS_COLLECTION.into(),
                }),
            )?)
        }()?;
        Ok(Self {
            inner: SuggestStoreInner::new(path, settings_client),
        })
    }

    /// Queries the database for suggestions.
    pub fn query(&self, query: SuggestionQuery) -> SuggestApiResult<Vec<Suggestion>> {
        Ok(self.inner.query(query)?)
    }

    /// Interrupts any ongoing queries.
    ///
    /// This should be called when the user types new input into the address
    /// bar, to ensure that they see fresh suggestions as they type. This
    /// method does not interrupt any ongoing ingests.
    pub fn interrupt(&self) {
        self.inner.interrupt()
    }

    /// Ingests new suggestions from Remote Settings.
    pub fn ingest(&self, constraints: SuggestIngestionConstraints) -> SuggestApiResult<()> {
        Ok(self.inner.ingest(constraints)?)
    }

    /// Removes all content from the database.
    pub fn clear(&self) -> SuggestApiResult<()> {
        Ok(self.inner.clear()?)
    }
}

/// Constraints limit which suggestions to ingest from Remote Settings.
#[derive(Clone, Default, Debug)]
pub struct SuggestIngestionConstraints {
    /// The approximate maximum number of suggestions to ingest. Set to [`None`]
    /// for "no limit".
    ///
    /// Because of how suggestions are partitioned in Remote Settings, this is a
    /// soft limit, and the store might ingest more than requested.
    pub max_suggestions: Option<u64>,
}

/// The implementation of the store. This is generic over the Remote Settings
/// client, and is split out from the concrete [`SuggestStore`] for testing
/// with a mock client.
pub(crate) struct SuggestStoreInner<S> {
    path: PathBuf,
    dbs: OnceCell<SuggestStoreDbs>,
    settings_client: S,
}

impl<S> SuggestStoreInner<S> {
    fn new(path: impl AsRef<Path>, settings_client: S) -> Self {
        Self {
            path: path.as_ref().into(),
            dbs: OnceCell::new(),
            settings_client,
        }
    }

    /// Returns this store's database connections, initializing them if
    /// they're not already open.
    fn dbs(&self) -> Result<&SuggestStoreDbs> {
        self.dbs
            .get_or_try_init(|| SuggestStoreDbs::open(&self.path))
    }

    fn query(&self, query: SuggestionQuery) -> Result<Vec<Suggestion>> {
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

    fn interrupt(&self) {
        if let Some(dbs) = self.dbs.get() {
            // Only interrupt if the databases are already open.
            dbs.reader.interrupt_handle.interrupt();
        }
    }

    fn clear(&self) -> Result<()> {
        self.dbs()?.writer.write(|dao| dao.clear())
    }
}

impl<S> SuggestStoreInner<S>
where
    S: SuggestRemoteSettingsClient,
{
    fn ingest(&self, constraints: SuggestIngestionConstraints) -> Result<()> {
        let writer = &self.dbs()?.writer;

        let mut options = GetItemsOptions::new();
        // Remote Settings returns records in descending modification order
        // (newest first), but we want them in ascending order (oldest first),
        // so that we can eventually resume downloading where we left off.
        options.sort("last_modified", SortOrder::Ascending);
        if let Some(last_ingest) = writer.read(|dao| dao.get_meta::<u64>(LAST_INGEST_META_KEY))? {
            // Only download changes since our last ingest. If our last ingest
            // was interrupted, we'll pick up where we left off.
            options.gt("last_modified", last_ingest.to_string());
        }
        if let Some(max_suggestions) = constraints.max_suggestions {
            // Each record's attachment has 200 suggestions, so download enough
            // records to cover the requested maximum.
            let max_records = (max_suggestions.saturating_sub(1) / SUGGESTIONS_PER_ATTACHMENT) + 1;
            options.limit(max_records);
        }

        let records = self
            .settings_client
            .get_records_with_options(&options)?
            .data;
        for record in &records {
            match record {
                SuggestRecord::Typed(TypedSuggestRecord::Data {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    let suggestions = self
                        .settings_client
                        .get_data_attachment(&attachment.location)?
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
                        dao.put_meta(LAST_INGEST_META_KEY, last_modified)?;

                        Ok(())
                    })?;
                }
                SuggestRecord::Untyped {
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
                        dao.put_meta(LAST_INGEST_META_KEY, last_modified)?;
                        Ok(())
                    })?;
                }
                SuggestRecord::Typed(TypedSuggestRecord::Icon {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    let Some(icon_id) = record_id.as_icon_id() else {
                        continue
                    };
                    let data = self
                        .settings_client
                        .get_icon_attachment(&attachment.location)?;
                    writer.write(|dao| {
                        dao.insert_icon(icon_id, &data)?;
                        dao.put_meta(LAST_INGEST_META_KEY, last_modified)?;
                        Ok(())
                    })?;
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

/// Holds a store's open connections to the Suggest database.
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::{cell::RefCell, collections::HashMap};

    use anyhow::Context;
    use parking_lot::Once;
    use serde_json::json;

    use crate::rs::{DownloadedSuggestDataAttachment, SuggestRemoteSettingsRecords};

    /// A snapshot containing fake Remote Settings records and attachments for
    /// the store to ingest. We use snapshots to test the store's behavior in a
    /// data-driven way.
    struct Snapshot {
        records: SuggestRemoteSettingsRecords,
        data: HashMap<&'static str, DownloadedSuggestDataAttachment>,
    }

    impl Snapshot {
        /// Creates a snapshot from a JSON value that represents a collection of
        /// Suggest Remote Settings records.
        ///
        /// You can use the [`serde_json::json!`] macro to construct the JSON
        /// value, then pass it to this function. It's easier to use the
        /// `Snapshot::with_records(json!(...))` idiom than to construct the
        /// nested `SuggestRemoteSettingsRecords` structure by hand.
        fn with_records(value: serde_json::Value) -> anyhow::Result<Self> {
            Ok(Self {
                records: serde_json::from_value(value)
                    .context("Couldn't create snapshot with Remote Settings records")?,
                data: HashMap::new(),
            })
        }

        /// Adds a data attachment with one or more suggestions to the snapshot.
        fn with_data(
            mut self,
            location: &'static str,
            value: serde_json::Value,
        ) -> anyhow::Result<Self> {
            self.data.insert(
                location,
                serde_json::from_value(value)
                    .context("Couldn't add data attachment to snapshot")?,
            );
            Ok(self)
        }
    }

    /// A fake Remote Settings client that returns records and attachments from
    /// a snapshot.
    struct SnapshotSettingsClient {
        /// The current snapshot. You can modify it using
        /// [`RefCell::borrow_mut()`] to simulate remote updates in tests.
        snapshot: RefCell<Snapshot>,
    }

    impl SnapshotSettingsClient {
        /// Creates a client with an initial snapshot.
        fn with_snapshot(snapshot: Snapshot) -> Self {
            Self {
                snapshot: RefCell::new(snapshot),
            }
        }
    }

    impl SuggestRemoteSettingsClient for SnapshotSettingsClient {
        fn get_records_with_options(
            &self,
            _options: &GetItemsOptions,
        ) -> Result<SuggestRemoteSettingsRecords> {
            Ok(self.snapshot.borrow().records.clone())
        }

        fn get_data_attachment(&self, location: &str) -> Result<DownloadedSuggestDataAttachment> {
            Ok(self
                .snapshot
                .borrow()
                .data
                .get(location)
                .unwrap_or_else(|| {
                    unreachable!("Unexpected request for data attachment `{}`", location)
                })
                .clone())
        }

        fn get_icon_attachment(&self, location: &str) -> Result<Vec<u8>> {
            unreachable!("Unexpected request for icon attachment `{}`", location)
        }
    }

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

        let snapshot = Snapshot::with_records(json!({
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
        }))?
        .with_data(
            "data-1.json",
            json!([{
                "id": 0,
                "advertiser": "Los Pollos Hermanos",
                "iab_category": "8 - Food & Drink",
                "keywords": ["lo", "los", "los p", "los pollos", "los pollos h", "los pollos hermanos"],
                "title": "Los Pollos Hermanos - Albuquerque",
                "url": "https://www.lph-nm.biz",
                "icon": "5678",
            }]),
        )?;

        let store = SuggestStoreInner::new(
            "file:ingest?mode=memory&cache=shared",
            SnapshotSettingsClient::with_snapshot(snapshot),
        );

        store.ingest(SuggestIngestionConstraints::default())?;

        store.dbs()?.reader.read(|dao| {
            assert_eq!(dao.get_meta::<u64>(LAST_INGEST_META_KEY)?, Some(15));
            assert_eq!(
                dao.fetch_by_keyword("lo")?,
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
        })?;

        Ok(())
    }
}
