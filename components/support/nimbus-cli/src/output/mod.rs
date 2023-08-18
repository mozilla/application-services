// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub(crate) mod deeplink;
mod features;
mod fetch;
mod fml_cli;
pub(crate) mod info;
#[cfg(feature = "server")]
pub(crate) mod server;

pub(crate) use fml_cli::fml_cli;
