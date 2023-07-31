use std::ops::Deref;

use remote_settings::{Attachment, GetItemsOptions};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as RusqliteResult;
use serde::Deserialize;

use crate::Result;
use serde::Deserializer;

/// The Suggest Remote Settings collection name.
pub(crate) const REMOTE_SETTINGS_COLLECTION: &str = "quicksuggest";

/// The maximum number of suggestions in a Suggest record's attachment.
///
/// This should be the same as the `BUCKET_SIZE` constant in the
/// `mozilla-services/quicksuggest-rs` repo.
pub(crate) const SUGGESTIONS_PER_ATTACHMENT: u64 = 200;

/// A trait for a client that downloads suggestions from Remote Settings.
///
/// This trait lets tests use a mock client.
pub(crate) trait SuggestRemoteSettingsClient {
    /// Fetches records from the Suggest Remote Settings collection.
    fn get_records_with_options(
        &self,
        options: &GetItemsOptions,
    ) -> Result<SuggestRemoteSettingsRecords>;

    /// Fetches a data attachment with suggestions to ingest from the Suggest
    /// Remote Settings collection.
    fn get_data_attachment(&self, location: &str) -> Result<DownloadedSuggestDataAttachment>;

    /// Fetches an icon attachment from the Suggest Remote Settings collection.
    fn get_icon_attachment(&self, location: &str) -> Result<Vec<u8>>;
}

impl SuggestRemoteSettingsClient for remote_settings::Client {
    fn get_records_with_options(
        &self,
        options: &GetItemsOptions,
    ) -> Result<SuggestRemoteSettingsRecords> {
        Ok(self.get_records_raw_with_options(options)?.json()?)
    }

    fn get_data_attachment(&self, location: &str) -> Result<DownloadedSuggestDataAttachment> {
        Ok(self.get_attachment(location)?.json()?)
    }

    fn get_icon_attachment(&self, location: &str) -> Result<Vec<u8>> {
        Ok(self.get_attachment(location)?.body)
    }
}

/// The response body for a Suggest Remote Settings collection request.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SuggestRemoteSettingsRecords {
    pub data: Vec<SuggestRecord>,
}

/// A record with a known or an unknown type, or a tombstone, in the Suggest
/// Remote Settings collection.
///
/// Because `#[serde(other)]` doesn't support associated data
/// (serde-rs/serde#1973), we can't define variants for all the known types and
/// the unknown type in the same enum. Instead, we have this "outer", untagged
/// `SuggestRecord` with the "unknown type" variant, and an "inner", internally
/// tagged `TypedSuggestRecord` with all the "known type" variants.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum SuggestRecord {
    /// A record with a known type.
    Typed(TypedSuggestRecord),

    /// A tombstone, or a record with an unknown type, that we don't know how
    /// to ingest.
    ///
    /// Tombstones only have these three fields, with `deleted` set to `true`.
    /// Records with unknown types have `deleted` set to `false`, and may
    /// contain other fields that we ignore.
    Untyped {
        id: SuggestRecordId,
        last_modified: u64,
        #[serde(default)]
        deleted: bool,
    },
}

/// A record that we know how to ingest from Remote Settings.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub(crate) enum TypedSuggestRecord {
    #[serde(rename = "icon")]
    Icon {
        id: SuggestRecordId,
        last_modified: u64,
        attachment: Attachment,
    },
    #[serde(rename = "data")]
    Data {
        id: SuggestRecordId,
        last_modified: u64,
        attachment: Attachment,
    },
}

/// Represents either a single value, or a list of values. This is used to
/// deserialize downloaded data attachments.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum OneOrMany<T> {
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

/// The contents of a downloaded [`TypedSuggestRecord::Data`] attachment.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct DownloadedSuggestDataAttachment(pub OneOrMany<DownloadedSuggestion>);

/// The ID of a record in the Suggest Remote Settings collection.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub(crate) struct SuggestRecordId(String);

impl SuggestRecordId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// If this ID is for an icon record, extracts and returns the icon ID.
    ///
    /// The icon ID is the primary key for an ingested icon. Downloaded
    /// suggestions also reference these icon IDs, in
    /// [`DownloadedSuggestion::icon_id`].
    pub fn as_icon_id(&self) -> Option<&str> {
        self.0.strip_prefix("icon-")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct DownloadedSuggestionCommonDetails {
    pub keywords: Vec<String>,
    pub title: String,
    pub url: String,
    #[serde(rename = "icon")]
    pub icon_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct DownloadedAmpSuggestion {
    #[serde(flatten)]
    pub common_details: DownloadedSuggestionCommonDetails,
    pub advertiser: String,
    #[serde(rename = "id")]
    pub block_id: i32,
    pub iab_category: String,
    pub click_url: String,
    pub impression_url: String,
}

/// Provider Types
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum SuggestionProvider {
    Amp = 1,
    Wikipedia = 2,
}

impl FromSql for SuggestionProvider {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        u8::try_from(v)
            .ok()
            .and_then(SuggestionProvider::from_u8)
            .ok_or_else(|| FromSqlError::OutOfRange(v))
    }
}

impl SuggestionProvider {
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(SuggestionProvider::Amp),
            2 => Some(SuggestionProvider::Wikipedia),
            _ => None,
        }
    }
}

impl ToSql for SuggestionProvider {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

/// A suggestion to ingest from a downloaded Remote Settings attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DownloadedSuggestion {
    Amp(DownloadedAmpSuggestion),
    Wikipedia(DownloadedSuggestionCommonDetails),
}

impl DownloadedSuggestion {
    /// Returns the suggestion fields that are common to AMP and
    /// Wikipedia suggestions.
    pub fn common_details(&self) -> &DownloadedSuggestionCommonDetails {
        match self {
            Self::Amp(DownloadedAmpSuggestion { common_details, .. }) => common_details,
            Self::Wikipedia(common_details) => common_details,
        }
    }

    pub fn provider(&self) -> SuggestionProvider {
        match self {
            DownloadedSuggestion::Amp(_) => SuggestionProvider::Amp,
            DownloadedSuggestion::Wikipedia(_) => SuggestionProvider::Wikipedia,
        }
    }
}

impl<'de> Deserialize<'de> for DownloadedSuggestion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<DownloadedSuggestion, D::Error>
    where
        D: Deserializer<'de>,
    {
        // AMP and Wikipedia suggestions conform to the same JSON schema, but
        // we want to represent them separately. To distinguish between the two,
        // we use an "untagged" outer enum and a "tagged" inner enum.
        //
        // Wikipedia suggestions always use the `"Wikipedia"` advertiser, so
        // they'll deserialize successfully into the `KnownTag` variant.
        // AMP suggestions will try the `KnownTag` variant first, fail, then
        // try the `UnknownTag` variant and succeed.
        //
        // This strategy is an implementation detail, so we turn the nested
        // enums into a friendlier `DownloadedAmpSuggestion` enum after
        // deserializing.

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum UntaggedDownloadedSuggestion {
            KnownTag(TaggedDownloadedSuggestion),
            UnknownTag(DownloadedAmpSuggestion),
        }

        #[derive(Deserialize)]
        #[serde(tag = "advertiser")]
        enum TaggedDownloadedSuggestion {
            #[serde(rename = "Wikipedia")]
            Wikipedia(DownloadedSuggestionCommonDetails),
        }

        Ok(
            match UntaggedDownloadedSuggestion::deserialize(deserializer)? {
                UntaggedDownloadedSuggestion::KnownTag(TaggedDownloadedSuggestion::Wikipedia(
                    common_details,
                )) => Self::Wikipedia(common_details),
                UntaggedDownloadedSuggestion::UnknownTag(common_details) => {
                    Self::Amp(common_details)
                }
            },
        )
    }
}
