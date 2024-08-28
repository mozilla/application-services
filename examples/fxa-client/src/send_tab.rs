/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Args, Subcommand};
use fxa_client::{FirefoxAccount, IncomingDeviceCommand};

use crate::{persist_fxa_state, Result};

#[derive(Args)]
pub struct SendTabArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Perform a single poll for tabs sent to this device
    Poll,
    /// Send a tab to another device
    Send {
        /// Device ID (use the `devices` command to list)
        device_id: String,
        title: String,
        url: String,
    },
    /// Close an open tab on another device
    Close {
        device_id: String,
        urls: Vec<String>,
    },
}

pub fn run(account: &FirefoxAccount, args: SendTabArgs) -> Result<()> {
    match args.command {
        Command::Poll => poll(account),
        Command::Send {
            device_id,
            title,
            url,
        } => send(account, device_id, title, url),
        Command::Close { device_id, urls } => close(account, device_id, urls),
    }
}

fn poll(account: &FirefoxAccount) -> Result<()> {
    println!("Polling for send-tab events.  Ctrl-C to cancel");
    loop {
        let events = account.poll_device_commands().unwrap_or_default(); // Ignore 404 errors for now.
        persist_fxa_state(account)?;
        if !events.is_empty() {
            for e in events {
                match e {
                    IncomingDeviceCommand::TabReceived { sender, payload } => {
                        let tab = &payload.entries[0];
                        match sender {
                            Some(ref d) => {
                                println!("Tab received from {}: {}", d.display_name, tab.url)
                            }
                            None => println!("Tab received: {}", tab.url),
                        };
                    }
                    IncomingDeviceCommand::TabsClosed { .. } => continue,
                }
            }
        }
    }
}

fn send(account: &FirefoxAccount, device_id: String, title: String, url: String) -> Result<()> {
    account.send_single_tab(&device_id, &title, &url)?;
    println!("Tab sent!");
    Ok(())
}

fn close(account: &FirefoxAccount, device_id: String, urls: Vec<String>) -> Result<()> {
    account.close_tabs(&device_id, urls)?;
    println!("Tabs closed!");
    Ok(())
}
