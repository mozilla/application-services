/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use uniffi::Record;

/// Configuration for the Wrexham widget client.
#[derive(Clone, Debug, Record)]
pub struct WrexhamConfig {
    /// The base host for the merino endpoints. Defaults to the production host if not set.
    pub base_host: Option<String>,
}

/// Options for a matches request. Omitted fields are not sent to merino.
#[derive(Clone, Debug, Default, Record)]
pub struct WrexhamMatchesOptions {
    /// The maximum number of previous and upcoming matches to return. Merino accepts 1 to 5,
    /// and returns 5 if not set. Current matches aren't limited.
    pub limit: Option<u32>,
}
