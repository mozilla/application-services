mod db;
mod error;
mod provider;
mod schema;

use serde_derive::*;

pub use error::SuggestError;
pub use provider::{IngestLimits, SuggestionProvider};

pub type Result<T, E = error::Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RemoteRecordId(String);

impl RemoteRecordId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
pub struct RemoteSuggestion {
    #[serde(rename = "id")]
    pub block_id: i64,
    pub advertiser: String,
    pub iab_category: String,
    pub keywords: Vec<String>,
    pub title: String,
    pub url: String,
    pub impression_url: Option<String>,
    pub click_url: Option<String>,
}

pub struct Suggestion {
    pub block_id: String,
    pub advertiser: String,
    pub iab_category: String,
    pub full_keyword: String,
    pub title: String,
    pub url: String,
    pub impression_url: Option<String>,
    pub click_url: Option<String>,
}

uniffi::include_scaffolding!("suggest");
