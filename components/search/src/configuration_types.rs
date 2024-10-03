/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the structures that we use for serde_json to parse
//! the search configuration.

use crate::{SearchEngineClassification, SearchUrlParam};
use serde::Deserialize;

/// The list of possible submission methods for search engine urls.
#[derive(Debug, uniffi::Enum, PartialEq, Deserialize, Clone, Default)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum JSONEngineMethod {
    Post = 2,
    #[serde(other)]
    #[default]
    Get = 1,
}

impl JSONEngineMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            JSONEngineMethod::Get => "GET",
            JSONEngineMethod::Post => "POST",
        }
    }
}

/// Defines an individual search engine URL. This is defined separately to
/// `types::SearchEngineUrl` as various fields may be optional in the supplied
/// configuration.
#[derive(Debug, uniffi::Record, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineUrl {
    /// The PrePath and FilePath of the URL. May include variables for engines
    /// which have a variable FilePath, e.g. `{searchTerm}` for when a search
    /// term is within the path of the url.
    pub base: String,

    /// The HTTP method to use to send the request (`GET` or `POST`).
    /// If the engine definition has not specified the method, it defaults to GET.
    pub method: Option<JSONEngineMethod>,

    /// The parameters for this URL.
    pub params: Option<Vec<SearchUrlParam>>,

    /// The name of the query parameter for the search term. Automatically
    /// appended to the end of the query. This may be skipped if `{searchTerm}`
    /// is included in the base.
    pub search_term_param_name: Option<String>,
}

/// Reflects `types::SearchEngineUrls`, but using `EngineUrl`.
#[derive(Debug, uniffi::Record, PartialEq, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineUrls {
    /// The URL to use for searches.
    pub search: JSONEngineUrl,

    /// The URL to use for suggestions.
    pub suggestions: Option<JSONEngineUrl>,

    /// The URL to use for trending suggestions.
    pub trending: Option<JSONEngineUrl>,
}

/// Represents the engine base section of the configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineBase {
    /// A list of aliases for this engine.
    pub aliases: Option<Vec<String>>,

    /// The character set this engine uses for queries. Defaults to 'UTF=8' if not set.
    pub charset: Option<String>,

    /// The classification of search engine according to the main search types
    /// (e.g. general, shopping, travel, dictionary). Currently, only marking as
    /// a general search engine is supported.
    pub classification: SearchEngineClassification,

    /// The user visible name for the search engine.
    pub name: String,

    /// The partner code for the engine. This will be inserted into parameters
    /// which include `{partnerCode}`.
    pub partner_code: Option<String>,

    /// The URLs associated with the search engine.
    pub urls: JSONEngineUrls,
}

/// Represents an individual engine record in the configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineRecord {
    pub identifier: String,
    pub base: JSONEngineBase,
}

/// Represents the default engines record.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONDefaultEnginesRecord {
    pub global_default: String,
    pub global_default_private: Option<String>,
}

/// Represents the engine orders record.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineOrdersRecord {
    // TODO: Implementation.
}

/// Represents an individual record in the raw search configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "recordType", rename_all = "camelCase")]
pub(crate) enum JSONSearchConfigurationRecords {
    DefaultEngines(JSONDefaultEnginesRecord),
    Engine(Box<JSONEngineRecord>),
    EngineOrders(JSONEngineOrdersRecord),
    // Include some flexibilty if we choose to add new record types in future.
    // Current versions of the application receiving the configuration will
    // ignore the new record types.
    #[serde(other)]
    Unknown,
}

/// Represents the search configuration as received from remote settings.
#[derive(Debug, Deserialize)]
pub(crate) struct JSONSearchConfiguration {
    pub data: Vec<JSONSearchConfigurationRecords>,
}
