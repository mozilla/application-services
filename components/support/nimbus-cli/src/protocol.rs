// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde_json::Value;

/// This is the protocol that each app understands.
///
/// It is sent to the apps via the start_app command.
///
/// Any change to this protocol requires changing the Kotlin and Swift code,
/// and perhaps the app code itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct StartAppProtocol<'a> {
    pub(crate) reset_db: bool,
    pub(crate) experiments: Option<&'a Value>,
    pub(crate) log_state: bool,
}
