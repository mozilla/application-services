/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// The possible application names.
#[derive(Debug, uniffi::Enum)]
pub enum SearchApplicationName {
    Firefox = 1,
    FirefoxAndroid = 2,
    FocusAndroid = 3,
    FirefoxIOS = 4,
    FocusIOS = 5,
}

impl SearchApplicationName {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchApplicationName::Firefox => "firefox",
            SearchApplicationName::FirefoxAndroid => "firefox-android",
            SearchApplicationName::FocusAndroid => "focus-android",
            SearchApplicationName::FirefoxIOS => "firefox-ios",
            SearchApplicationName::FocusIOS => "focus-ios",
        }
    }
}

/// The possible channels that may be in use.
#[derive(Debug, uniffi::Enum)]
pub enum SearchDistributionChannel {
    Default,
    Nightly,
    Aurora,
    Beta,
    Release,
    ESR,
}

impl SearchDistributionChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchDistributionChannel::Default => "default",
            SearchDistributionChannel::Nightly => "nightly",
            SearchDistributionChannel::Aurora => "aurora",
            SearchDistributionChannel::Beta => "beta",
            SearchDistributionChannel::Release => "release",
            SearchDistributionChannel::ESR => "esr",
        }
    }
}

/// The user's environment that is used for filtering the search configuration.
#[derive(Debug, uniffi::Record)]
pub struct SearchUserEnvironment {
    /// The current locale of the application that the user is using.
    pub locale: String,

    /// The home region that the user is currently identified as being within.
    /// On desktop & android there is a 14 day lag after detecting a region
    /// change before the home region changes. TBD: iOS?
    pub region: String,

    /// The current distribution channel.
    /// Use `default` for a self-build or an unknown channel.
    pub channel: SearchDistributionChannel,

    /// The distribution id for the user's build.
    pub distribution_id: String,

    /// The search related experiment id that the user is included within. On
    /// desktop this is the `searchConfiguration.experiment` variable.
    pub experiment: String,

    /// The application name that the user is using.
    pub app_name: SearchApplicationName,

    /// The application version that the user is using.
    pub version: String,
}

/// Parameter definitions for search engine URLs. The name property is always
/// specified, along with one of value, experiment_config or search_access_point.
#[derive(Debug, uniffi::Record)]
pub struct SearchUrlParam {
    /// The name of the parameter in the url.
    pub name: String,
    /// The parameter value, this may be a static value, or additionally contain
    /// a parameter replacement, e.g. `{inputEncoding}`. For the partner code
    /// parameter, this field should be `{partnerCode}`.
    pub value: Option<String>,
    /// The value for the parameter will be derived from the equivalent experiment
    /// configuration value.
    /// Only desktop uses this currently.
    pub experiment_config: Option<String>,
}

/// Defines an individual search engine URL.
#[derive(Debug, uniffi::Record)]
pub struct SearchEngineUrl {
    /// The PrePath and FilePath of the URL. May include variables for engines
    /// which have a variable FilePath, e.g. `{searchTerm}` for when a search
    /// term is within the path of the url.
    pub base: String,

    /// The HTTP method to use to send the request (`GET` or `POST`).
    /// If not specified, defaults to GET.
    pub method: Option<String>,

    /// The parameters for this URL.
    pub params: Option<Vec<SearchUrlParam>>,

    /// The name of the query parameter for the search term. Automatically
    /// appended to the end of the query. This may be skipped if `{searchTerm}`
    /// is included in the base.
    pub search_term_param_name: Option<String>,
}

/// The URLs associated with the search engine.
#[derive(Debug, uniffi::Record)]
pub struct SearchEngineUrls {
    /// The URL to use for searches.
    pub search: SearchEngineUrl,

    /// The URL to use for suggestions.
    pub suggestions: Option<SearchEngineUrl>,

    /// The URL to use for trending suggestions.
    pub trending: Option<SearchEngineUrl>,
}

/// A definition for an individual search engine to be presented to the user.
#[derive(Debug, uniffi::Record)]
pub struct SearchEngineDefinition {
    /// An optional list of aliases for this engine.
    pub aliases: Option<Vec<String>>,

    /// The classification of search engine according to the main search types
    /// (e.g. general, shopping, travel, dictionary). Currently, only marking as
    /// a general search engine is supported.
    /// On Android, only general search engines may be selected as "default"
    /// search engines.
    pub classification: String,

    /// The identifier of the search engine. This is used as an internal
    /// identifier, e.g. for saving the user's settings for the engine. It is
    /// also used to form the base telemetry id and may be extended by telemetrySuffix.
    pub identifier: String,

    /// The user visible name of the search engine.
    pub name: String,

    /// The partner code for the engine. This will be inserted into parameters
    /// which include `{partnerCode}`.
    pub partner_code: Option<String>,

    /// Optional suffix that is appended to the search engine identifier
    /// following a dash, i.e. `<identifier>-<suffix>`
    pub telemetry_suffix: Option<String>,

    /// The URLs associated with the search engine.
    pub urls: SearchEngineUrls,
}

/// Details of the search engines to display to the user, generated as a result
/// of processing the search configuration.
#[derive(Debug, uniffi::Record)]
pub struct RefinedConfig {
    /// A list of engines in their default sort order. The default engine should
    /// not be assumed from this order.
    pub engines: Vec<SearchEngineDefinition>,

    /// The identifier of the engine that should be used for the application
    /// default engine.
    pub app_default_engine_id: String,

    /// If specified, the identifier of the engine that should be used for the
    /// application default engine in private browsing mode.
    /// Only desktop uses this currently.
    pub app_default_private_engine_id: Option<String>,
}
