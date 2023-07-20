/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use remote_settings::RemoteSettingsConfig;
use serde::Deserialize;

mod db;
mod error;
mod keyword;
mod schema;
mod store;

pub use error::SuggestApiError;
pub use store::{SuggestIngestionConstraints, SuggestStore};

pub(crate) type Result<T> = std::result::Result<T, error::Error>;
pub type SuggestApiResult<T> = std::result::Result<T, error::SuggestApiError>;

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

/// A suggestion to ingest from a downloaded Remote Settings attachment.
#[derive(Debug, Deserialize)]
pub(crate) struct DownloadedSuggestion {
    #[serde(rename = "id")]
    pub block_id: i64,
    pub advertiser: String,
    pub iab_category: String,
    pub keywords: Vec<String>,
    pub title: String,
    pub url: String,
    #[serde(rename = "icon")]
    pub icon_id: String,
    pub impression_url: Option<String>,
    pub click_url: Option<String>,
}

/// A suggestion from the database to show in the address bar.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Suggestion {
    pub block_id: i64,
    pub advertiser: String,
    pub iab_category: String,
    pub is_sponsored: bool,
    pub full_keyword: String,
    pub title: String,
    pub url: String,
    pub icon: Option<Vec<u8>>,
    pub impression_url: Option<String>,
    pub click_url: Option<String>,
}

/// A query for suggestions to show in the address bar.
#[derive(Debug, Default)]
pub struct SuggestionQuery {
    pub keyword: String,
    pub include_sponsored: bool,
    pub include_non_sponsored: bool,
}

uniffi::include_scaffolding!("suggest");
