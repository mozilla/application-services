/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module defines the structures that we use for serde_json to parse
//! the search configuration.

use crate::{SearchApplicationName, SearchEngineClassification, SearchUrlParam};
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

/// Specifies details of possible user environments that the engine or variant
/// applies to.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONVariantEnvironment {
    /// Indicates that this section applies to all regions and locales. May be
    /// modified by excluded_regions/excluded_locales.
    #[serde(default)]
    pub all_regions_and_locales: bool,

    /// A vector of locales that this section should be excluded from. 'default'
    /// will apply to situations where we have not been able to detect the user's
    /// locale.
    #[serde(default)]
    pub excluded_locales: Vec<String>,

    /// A vector of regions that this section should be excluded from. 'default'
    /// will apply to situations where we have not been able to detect the user's
    /// region.
    #[serde(default)]
    pub excluded_regions: Vec<String>,

    /// A vector of locales that this section applies to. 'default' will apply
    /// to situations where we have not been able to detect the user's locale.
    #[serde(default)]
    pub locales: Vec<String>,

    /// A vector of regions that this section applies to. 'default' will apply
    /// to situations where we have not been able to detect the user's region.
    #[serde(default)]
    pub regions: Vec<String>,

    /// A vector of distribution identifiers that this section applies to.
    #[serde(default)]
    pub distributions: Vec<String>,

    /// A vector of distributions that this section should be excluded from.
    #[serde(default)]
    pub excluded_distributions: Vec<String>,

    /// A vector of applications that this applies to.
    #[serde(default)]
    pub applications: Vec<SearchApplicationName>,
    // TODO: Implement these.
    // pub channels: Option<Vec<String>,
    // pub experiment: Option<String>,
    // pub min_version: Option<String>,
    // pub max_version: Option<String>,
    // pub device_type: Option<String>,
}

/// Describes an individual variant of a search engine.
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct JSONEngineVariant {
    /// Details of the possible user environments that this variant applies to.
    pub environment: JSONVariantEnvironment,
}

/// Represents an individual engine record in the configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JSONEngineRecord {
    /// The identiifer for the search engine.
    pub identifier: String,

    /// The base information of the search engine, may be extended by the
    /// variants.
    pub base: JSONEngineBase,

    /// Describes variations of this search engine that may occur depending on
    /// the user's environment. The last variant that matches the user's
    /// environment will be applied to the engine, subvariants may also be applied.
    pub variants: Vec<JSONEngineVariant>,
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
