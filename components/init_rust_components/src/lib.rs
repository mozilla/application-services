/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

uniffi::setup_scaffolding!();

/// Initialization of the megazord crate. Must be called before any other calls to rust components.
#[uniffi::export]
pub fn initialize() {
    // this is currently empty, we will add nss initialization code here in a next step
}
