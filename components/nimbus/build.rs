/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

pub fn main() {
    #[cfg(feature = "stateful-uniffi-bindings")]
    uniffi::generate_scaffolding("./src/nimbus.udl").unwrap();

    #[cfg(not(feature = "stateful-uniffi-bindings"))]
    uniffi::generate_scaffolding("./src/cirrus.udl").unwrap();
}
