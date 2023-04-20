// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct Cli {
    /// The app name according to Nimbus.
    #[arg(short, long, value_name = "APP")]
    pub(crate) app: String,

    /// The channel according to Nimbus. This determines which app to talk to.
    #[arg(short, long, value_name = "CHANNEL")]
    pub(crate) channel: String,

    /// The device id of the simulator, emulator or device.
    #[arg(short, long, value_name = "DEVICE_ID")]
    pub(crate) device_id: Option<String>,

    #[command(subcommand)]
    pub(crate) command: CliCommand,
}

#[derive(Subcommand, Clone)]
pub(crate) enum CliCommand {
    /// Enroll into an experiment or a rollout
    Enroll {
        #[arg(value_name = "SLUG")]
        experiment: String,
        #[arg(short, long, value_name = "BRANCH")]
        branch: String,

        /// Preserves the original experiment targeting
        #[arg(short, long, default_value = "false")]
        preserve_targeting: bool,

        /// Resets the app back to its initial state before launching
        #[arg(long)]
        reset: bool,
    },

    /// List the experiments from a server
    List {
        /// A server slug e.g. preview, release, stage, stage/preview
        server: Option<String>,
    },

    /// Reset the app back to its just installed state
    ResetApp,

    /// Unenroll from all experiments and rollouts
    Unenroll,
}
