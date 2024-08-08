/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use remote_settings::{RemoteSettingsConfig, RemoteSettingsServer};
#[cfg(feature = "benchmark_api")]
pub mod benchmarks;
mod config;
mod db;
mod error;
mod keyword;
pub mod pocket;
mod provider;
mod query;
mod rs;
mod schema;
mod store;
mod suggestion;
#[cfg(test)]
mod testing;
mod yelp;

pub use config::{SuggestGlobalConfig, SuggestProviderConfig};
pub use error::{Error, SuggestApiError};
pub use provider::SuggestionProvider;
pub use query::SuggestionQuery;
pub use store::{InterruptKind, SuggestIngestionConstraints, SuggestStore, SuggestStoreBuilder};
pub use suggestion::{raw_suggestion_url_matches, Suggestion};

pub(crate) type Result<T> = std::result::Result<T, Error>;
pub type SuggestApiResult<T> = std::result::Result<T, SuggestApiError>;

uniffi::use_udl_record!(remote_settings, RemoteSettingsConfig);
uniffi::use_udl_enum!(remote_settings, RemoteSettingsServer);
uniffi::setup_scaffolding!();
