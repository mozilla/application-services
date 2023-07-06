/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod devices;
mod send_tab;

use std::fs;

use clap::{Parser, Subcommand, ValueEnum};
use cli_support::fxa_creds::get_cli_fxa;
use fxa_client::{FirefoxAccount, FxaConfig, FxaServer};

static CREDENTIALS_PATH: &str = "credentials.json";
static CLIENT_ID: &str = "a2270f727f45f648";
static REDIRECT_URI: &str = "https://accounts.firefox.com/oauth/success/a2270f727f45f648";

use anyhow::Result;

#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// The FxA server to use
    #[arg(value_enum, default_value_t = Server::Release)]
    server: Server,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Server {
    /// Official server
    Release,
    /// China server
    China,
    /// stable dev sever
    Stable,
    /// staging dev sever
    Stage,
    /// local dev sever
    LocalDev,
}

#[derive(Subcommand)]
enum Command {
    Devices(devices::DeviceArgs),
    SendTab(send_tab::SendTabArgs),
    Disconnect,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    viaduct_reqwest::use_reqwest_backend();

    println!();
    let account = load_account(&cli)?;
    match cli.command {
        Command::Devices(args) => devices::run(&account, args),
        Command::SendTab(args) => send_tab::run(&account, args),
        Command::Disconnect => {
            account.disconnect()?;
            Ok(())
        }
    }?;

    Ok(())
}

fn load_account(cli: &Cli) -> Result<FirefoxAccount> {
    let config = FxaConfig {
        server: match cli.server {
            Server::Release => FxaServer::Release,
            Server::Stable => FxaServer::Stable,
            Server::Stage => FxaServer::Stage,
            Server::China => FxaServer::China,
            Server::LocalDev => FxaServer::LocalDev,
        },
        redirect_uri: REDIRECT_URI.into(),
        client_id: CLIENT_ID.into(),
        token_server_url_override: None,
    };
    get_cli_fxa(config, CREDENTIALS_PATH).map(|cli| cli.account)
}

pub fn persist_fxa_state(acct: &FirefoxAccount) -> Result<()> {
    let json = acct.to_json().unwrap();
    Ok(fs::write(CREDENTIALS_PATH, json)?)
}
