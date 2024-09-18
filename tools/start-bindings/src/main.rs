/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Parser, Subcommand};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Android {
        crate_name: String,
        description: String,
    },
    Ios {
        crate_name: String,
    },
    IosFocus {
        crate_name: String,
    },
}

fn main() {
    let args = Args::parse();
    let result = match args.command {
        Command::Android {
            crate_name,
            description,
        } => start_bindings::generate_android(crate_name, description),
        Command::Ios { crate_name } => start_bindings::generate_ios(crate_name),
        Command::IosFocus { crate_name } => start_bindings::generate_ios_focus(crate_name),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
