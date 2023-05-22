/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::ApiResult;

/// FxA storage layer
///
/// This is implemented on the foreign side and passed to the rust code in the `FirefoxAccount`
/// constructor.
pub trait FxaStorage {
    /// Is there saved state to load?
    ///
    /// This is called at startup to check if we should try to load the saved state.  If this
    /// returns true, the `FirefoxAccount` instance will invoke `load_state()` and into a loading
    /// state while that executes.
    fn has_saved_state(&self) -> bool;

    /// Load the saved state.
    ///
    /// Return the last `state` sent to `save_state`.
    fn load_state(&self) -> ApiResult<String>;

    /// Save the current FxA state
    ///
    /// Note: only one `save_state` call will be scheduled at once.  If a change happens
    /// while one `save_state` call is still executing, `fxa_client` will wait to send the second
    /// `save_state` call.
    fn save_state(&self, state: String) -> ApiResult<()>;
}
