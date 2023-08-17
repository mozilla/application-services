// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    ffi::{OsStr, OsString},
    path::Path,
    vec,
};

use anyhow::Result;
use nimbus_fml::command_line::do_main;

pub(crate) fn fml_cli(args: &Vec<OsString>, cwd: &Path) -> Result<bool> {
    // We prepend the string `nimbus-cli fml` to the args to pass to FML
    // because the clap uses the 0th argument for help messages.
    let first = OsStr::new("nimbus-cli fml").to_os_string();
    let mut cli_args = vec![&first];

    // To make this a little more ergonomic, if the user has just typed
    // `nimbus-cli fml`, then we can help them a little bit.
    let help = OsStr::new("--help").to_os_string();
    if args.is_empty() {
        cli_args.push(&help);
    } else {
        cli_args.extend(args);
    }
    do_main(cli_args, cwd)?;
    Ok(true)
}
