use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use once_cell::sync::OnceCell;
use remote_settings::{self, Attachment, GetItemsOptions, RemoteSettingsConfig, SortOrder};
use serde_derive::*;

use crate::{
    db::{ConnectionType, SuggestDb, LAST_FETCH_META_KEY},
    error::SuggestError,
    RemoteRecordId, RemoteSuggestion, Result, Suggestion,
};

const REMOTE_SETTINGS_SERVER_URL: &'static str = "https://firefox.settings.services.mozilla.com/v1";
const REMOTE_SETTINGS_DEFAULT_BUCKET: &'static str = "main";
const RS_COLLECTION: &'static str = "quicksuggest";

/// The provider is the entry point to the Suggest component. It incrementally
/// fetches suggestions from the Remote Settings service, stores them in a local
/// database, and returns them in response to user queries.
///
/// Your application should create a single provider, and manage it as a
/// singleton. The provider is thread-safe, and supports concurrent fetches and
/// ingests. We expect that your application will call `fetch` to show
/// suggestions as the user types into the address bar, and periodically call
/// `ingest` in the background to update the database with new suggestions from
/// Remote Settings.
///
/// The provider keeps track of the state needed to support incremental
/// ingestion, but doesn't schedule the ingestion work itself, because the
/// primitives for scheduling background work vary across platforms: Desktop
/// might use an idle timer to poll for changes, Android has `WorkManager`, and
/// iOS has `BGTaskScheduler`.
///
/// Ingestion limits can vary between platforms, too: a mobile browser on a
/// metered connection might want to request a small subset of the Suggest data
/// and fetch the rest later, while a Desktop browser on a fast link might
/// request the entire dataset on first launch.
pub struct SuggestionProvider {
    path: PathBuf,
    dbs: OnceCell<SuggestionProviderDbs>,
    settings_client: remote_settings::Client,
}

/// Limits for an ingestion from Remote Settings.
pub struct IngestLimits {
    /// The maximum number of records to request from Remote Settings.
    /// Each record has about 200 suggestions.
    pub records: Option<u64>,
}

impl SuggestionProvider {
    /// Creates a suggestion provider.
    pub fn new(path: &str) -> Result<Self, SuggestError> {
        Ok(Self::new_inner(path)?)
    }

    fn new_inner(path: impl AsRef<Path>) -> Result<Self> {
        let settings_client = remote_settings::Client::new(RemoteSettingsConfig {
            server_url: Some(REMOTE_SETTINGS_SERVER_URL.into()),
            bucket_name: Some(REMOTE_SETTINGS_DEFAULT_BUCKET.into()),
            collection_name: RS_COLLECTION.into(),
        })?;
        Ok(Self {
            path: path.as_ref().into(),
            dbs: OnceCell::new(),
            settings_client,
        })
    }

    /// Returns this provider's database connections, initializing them if
    /// they're not already open.
    fn dbs(&self) -> Result<&SuggestionProviderDbs> {
        self.dbs
            .get_or_try_init(|| SuggestionProviderDbs::open(&self.path))
    }

    /// Queries the database for suggestions that match the `keyword`.
    pub fn query(&self, keyword: &str) -> Result<Vec<Suggestion>, SuggestError> {
        Ok(self.dbs()?.reader.fetch_by_keyword(keyword)?)
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
    pub fn ingest(&self, limits: &IngestLimits) -> Result<(), SuggestError> {
        Ok(self.ingest_inner(limits)?)
    }

    fn ingest_inner(&self, limits: &IngestLimits) -> Result<()> {
        let writer = &self.dbs()?.writer;
        let scope = writer.interrupt_handle.begin_interrupt_scope()?;

        let mut options = GetItemsOptions::new();
        // Remote Settings returns records in descending modification order
        // (newest first), but we want them in ascending order (oldest first),
        // so that we can eventually resume fetching where we left off.
        options.sort("last_modified", SortOrder::Ascending);
        if let Some(last_fetch) = writer.get_meta::<u64>(LAST_FETCH_META_KEY)? {
            // Only fetch changes since our last fetch. If our last fetch was
            // interrupted, we'll pick up where we left off.
            options.gt("last_modified", last_fetch.to_string());
        }
        if let Some(records) = &limits.records {
            options.limit(*records);
        }

        scope.err_if_interrupted()?;
        let records = self
            .settings_client
            .get_records_raw_with_options(&options)?
            .json::<SuggestRecordsResponse>()?
            .data;
        for record in &records {
            scope.err_if_interrupted()?;
            match record {
                FetchedChange::Record(SuggestRecord::Data {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    // Drop any suggestions that we previously ingested from
                    // this record's attachment. Suggestions don't have a
                    // stable identifier, and determining which suggestions in
                    // the attachment actually changed is more complicated than
                    // dropping and re-ingesting all of them.
                    writer.drop(record_id)?;

                    // Ingest (or re-ingest) all suggestions in the attachment.
                    scope.err_if_interrupted()?;
                    let suggestions = self
                        .settings_client
                        .get_attachment(&attachment.location)?
                        .json::<SuggestAttachmentData>()?
                        .0;
                    writer.ingest(record_id, &suggestions)?;

                    // Advance the last fetch time, so that we can resume
                    // fetching after this record if we're interrupted.
                    writer.put_meta(LAST_FETCH_META_KEY, last_modified)?;
                }
                FetchedChange::Unknown {
                    id: record_id,
                    last_modified,
                    deleted,
                } if *deleted => {
                    // If the entire record was deleted, drop all its
                    // suggestions and advance the last fetch time.
                    match record_id.as_icon_id() {
                        Some(icon_id) => writer.drop_icon(icon_id)?,
                        None => writer.drop(record_id)?,
                    };
                    writer.put_meta(LAST_FETCH_META_KEY, last_modified)?
                }
                FetchedChange::Record(SuggestRecord::Icon {
                    id: record_id,
                    last_modified,
                    attachment,
                }) => {
                    let Some(icon_id) = record_id.as_icon_id() else {
                        continue
                    };
                    scope.err_if_interrupted()?;
                    let data = self
                        .settings_client
                        .get_attachment(&attachment.location)?
                        .body;
                    writer.put_icon(icon_id, &data)?;
                    writer.put_meta(LAST_FETCH_META_KEY, last_modified)?;
                }
                _ => continue,
            }
        }

        Ok(())
    }
}

struct SuggestionProviderDbs {
    /// A read-write connection used to update the database with new data.
    writer: SuggestDb,
    /// A read-only connection used to query the database.
    reader: SuggestDb,
}

impl SuggestionProviderDbs {
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

    #[test]
    fn is_thread_safe() {
        // Ensure that `SuggestionProvider` is usable with UniFFI, which
        // requires exposed interfaces to be `Send` and `Sync`.
        fn is_send_sync<T: Send + Sync>() {}
        is_send_sync::<SuggestionProvider>();
    }

    #[test]
    fn ingest() -> anyhow::Result<()> {
        viaduct_reqwest::use_reqwest_backend();

        let provider = SuggestionProvider::new("file:ingest?mode=memory&cache=shared")?;
        provider.ingest(&IngestLimits { records: Some(3) })?;
        Ok(())
    }
}
