/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Telemetry Methods
//!
//! This component does not currently submit telemetry via Glean, but it *does* gather
//! a small amount of telemetry about send-tab that the application may submit on its
//! behalf.

use crate::{ApiResult, Error, FirefoxAccount};
use error_support::handle_error;

impl FirefoxAccount {
    /// Collect and return telemetry about send-tab attempts.
    ///
    /// Applications that register the [`SendTab`](DeviceCapability::SendTab) capability
    /// should also arrange to submit "sync ping" telemetry. Calling this method will
    /// return a JSON string of telemetry data that can be incorporated into that ping.
    ///
    /// Sorry, this is not particularly carefully documented because it is intended
    /// as a stop-gap until we get native Glean support. If you know how to submit
    /// a sync ping, you'll know what to do with the contents of the JSON string.
    #[handle_error(Error)]
    pub fn gather_telemetry(&self) -> ApiResult<String> {
        self.internal.lock().gather_telemetry()
    }
}
