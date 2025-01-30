/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Args, Subcommand};
use fxa_client::FirefoxAccount;

use crate::{persist_fxa_state, Result};

#[derive(Args)]
pub struct DeviceArgs {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    List,
    SetName { name: String },
}

pub fn run(account: &FirefoxAccount, args: DeviceArgs) -> Result<()> {
    match args.command.unwrap_or(Command::List) {
        Command::List => list(account),
        Command::SetName { name } => set_name(account, name),
    }
}

fn list(account: &FirefoxAccount) -> Result<()> {
    for device in account.get_devices(false)? {
        println!("{}: {}", device.id, device.display_name);
    }
    Ok(())
}

fn set_name(account: &FirefoxAccount, name: String) -> Result<()> {
    account.set_device_name(&name)?;
    println!("Display name set to {name}");
    persist_fxa_state(account)?;
    Ok(())
}
