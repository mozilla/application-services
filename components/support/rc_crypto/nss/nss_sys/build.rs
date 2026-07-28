/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

fn main() {
    #[cfg(feature = "mozbuild-rustlib")]
    nss_build_common::link_nss_rustlib().unwrap();
    #[cfg(not(feature = "mozbuild-rustlib"))]
    nss_build_common::link_nss().unwrap();
}
