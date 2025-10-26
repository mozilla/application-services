/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod config_supplier;
mod uniffi_bindgen;

pub fn main() -> anyhow::Result<()> {
    uniffi_bindgen::run_main()
}
