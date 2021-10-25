/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
mod kotlin;

use crate::{Config, GenerateStructCmd, TargetLanguage};

pub(crate) fn generate_struct(config: Option<Config>, cmd: GenerateStructCmd) {
    match cmd.language {
        TargetLanguage::Kotlin => kotlin::generate_struct(config, cmd),
        _ => unimplemented!(),
    }
}
