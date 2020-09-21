/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use anyhow::Result;
use structopt::StructOpt;

// Note: this uses doc comments to generate the help text.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "autofill-utils", about = "Command-line utilities for autofill")]
pub struct Opts {
    #[structopt(
        name = "database_path",
        long,
        short = "d",
        default_value = "./autofill.db"
    )]
    /// Path to the database, which will be created if it doesn't exist.
    pub database_path: String,

    /// Leaves all logging disabled, which may be useful when evaluating perf
    #[structopt(name = "no-logging", long)]
    pub no_logging: bool,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, StructOpt)]
enum Command {
    #[structopt(name = "sync")]
    /// Syncs all or some engines.
    Sync {},
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    if !opts.no_logging {
        cli_support::init_trace_logging();
    }

    Ok(())
}
