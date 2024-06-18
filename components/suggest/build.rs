/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

fn main() {
    #[cfg(feature = "fakespot")]
    uniffi::generate_scaffolding("./src/suggest-fakespot.udl").unwrap();

    #[cfg(not(feature = "fakespot"))]
    uniffi::generate_scaffolding("./src/suggest.udl").unwrap();
}
