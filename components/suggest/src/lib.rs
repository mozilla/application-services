use serde_derive::*;

mod db;
mod error;
mod query;
mod schema;
mod store;

pub use error::SuggestApiError;
pub use query::SuggestionQuery;
pub use store::{IngestLimits, SuggestStore};

pub type Result<T, E = error::Error> = std::result::Result<T, E>;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct RemoteRecordId(String);

impl RemoteRecordId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_icon_id(&self) -> Option<&str> {
        self.0.strip_prefix("icon-")
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
    #[serde(rename = "icon")]
    pub icon_id: String,
    pub impression_url: Option<String>,
    pub click_url: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Suggestion {
    pub block_id: String,
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

uniffi::include_scaffolding!("suggest");
