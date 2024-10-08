/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use serde::{Deserialize, Serialize};

use crate::rs::DownloadedGlobalConfig;

/// Global Suggest configuration data.
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq, uniffi::Record)]
pub struct SuggestGlobalConfig {
    pub show_less_frequently_cap: i32,
}

impl From<&DownloadedGlobalConfig> for SuggestGlobalConfig {
    fn from(config: &DownloadedGlobalConfig) -> Self {
        Self {
            show_less_frequently_cap: config.configuration.show_less_frequently_cap,
        }
    }
}

/// Per-provider configuration data.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, uniffi::Enum)]
pub enum SuggestProviderConfig {
    Weather { min_keyword_length: i32 },
}
