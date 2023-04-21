// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    author,
    long_about = r#"Mozilla Nimbus' command line tool for mobile apps"#
)]
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
    /// Enroll into an experiment or a rollout.
    ///
    /// The experiment slug is a combination of the actual slug, and the server it came from.
    ///
    /// * `release`/`stage` determines the server.
    ///
    /// * `preview` selects the preview collection.
    ///
    /// These can be further combined: e.g. $slug, preview/$slug, stage/$slug, stage/preview/$slug
    Enroll {
        /// The experiment slug, including the server and collection.
        #[arg(value_name = "SLUG")]
        experiment: String,

        /// The branch slug.
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
