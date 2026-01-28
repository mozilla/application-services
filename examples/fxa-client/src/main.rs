/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod devices;
mod send_tab;

use std::fs;

use clap::{Parser, Subcommand, ValueEnum};
use cli_support::{fxa_creds, init_logging_with};
use fxa_client::{FirefoxAccount, FxaConfig, FxaServer};

static CREDENTIALS_FILENAME: &str = "credentials.json";
static CLIENT_ID: &str = "a2270f727f45f648";

use anyhow::{bail, Result};

#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// The FxA server to use
    #[clap(long)]
    server: Option<Server>,

    /// Custom FxA server URL
    #[clap(long)]
    custom_url: Option<String>,

    /// Request a session scope
    #[clap(long, short, action)]
    session_scope: bool,

    /// Print out log to the console.  The default level is WARN
    #[clap(long, short, action)]
    log: bool,

    /// Set the logging level to INFO
    #[clap(long, short, action)]
    info: bool,

    /// Set the logging level to DEBUG
    #[clap(long, short, action)]
    debug: bool,

    /// Set the logging level to TRACE
    #[clap(long, short, action)]
    trace: bool,

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
    /// custom sever URL
    Custom,
}

#[derive(Subcommand)]
enum Command {
    Devices(devices::DeviceArgs),
    SendTab(send_tab::SendTabArgs),
    /// Get a new access token for a scope
    ///
    /// This can be used to test the token exchange API for relay
    /// (https://bugzilla.mozilla.org/show_bug.cgi?id=2012143).
    ///
    /// * Start with a fresh account by using the `cargo fxa disconnect`
    /// * Run `cargo fxa --log --trace get-access-token https://identity.mozilla.com/apps/relay`.
    ///   * You should see a token exchange request in the trace logs.
    /// * Run `cargo fxa --log --trace get-access-token https://identity.mozilla.com/apps/relay`
    ///   again.
    ///   * This time there shouldn't be a token exchange request, since the refresh token now has
    ///     the relay scope.
    GetAccessToken {
        scope: String,
    },
    Disconnect,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    nss::ensure_initialized();
    viaduct_hyper::viaduct_init_backend_hyper()?;
    if cli.log {
        if cli.trace {
            init_logging_with("fxa_client=trace");
        } else if cli.debug {
            init_logging_with("fxa_client=debug");
        } else if cli.info {
            init_logging_with("fxa_client=info");
        } else {
            init_logging_with("fxa_client=warn");
        }
    }

    let scopes: &[&str] = if cli.session_scope {
        &[fxa_creds::SYNC_SCOPE, fxa_creds::SESSION_SCOPE]
    } else {
        &[fxa_creds::SYNC_SCOPE]
    };

    println!();
    let account = load_account(&cli, scopes)?;
    match cli.command {
        Command::Devices(args) => devices::run(&account, args),
        Command::SendTab(args) => send_tab::run(&account, args),
        Command::GetAccessToken { scope } => {
            println!("Requesting access token with scope: {scope}");
            account.get_access_token(&scope, false)?;
            println!("Saving account with updated token");
            persist_fxa_state(&account)?;
            println!("Success");
            Ok(())
        }
        Command::Disconnect => {
            account.disconnect();
            Ok(())
        }
    }?;

    Ok(())
}

impl Cli {
    fn server(&self) -> Result<FxaServer> {
        Ok(match &self.server {
            None => FxaServer::Release,
            Some(Server::Release) => FxaServer::Release,
            Some(Server::Stable) => FxaServer::Stable,
            Some(Server::Stage) => FxaServer::Stage,
            Some(Server::China) => FxaServer::China,
            Some(Server::LocalDev) => FxaServer::LocalDev,
            Some(Server::Custom) => FxaServer::Custom {
                url: match &self.custom_url {
                    Some(url) => url.clone(),
                    None => bail!("--custom-url missing"),
                },
            },
        })
    }
}

fn load_account(cli: &Cli, scopes: &[&str]) -> Result<FirefoxAccount> {
    let server = cli.server()?;
    let redirect_uri = format!("{}/oauth/success/a2270f727f45f648", server.content_url());
    let config = FxaConfig {
        server,
        redirect_uri,
        client_id: CLIENT_ID.into(),
        token_server_url_override: None,
    };
    fxa_creds::get_cli_fxa(config, &credentials_path(), scopes).map(|cli| cli.account)
}

pub fn persist_fxa_state(acct: &FirefoxAccount) -> Result<()> {
    let json = acct.to_json().unwrap();
    Ok(fs::write(credentials_path(), json)?)
}

fn credentials_path() -> String {
    cli_support::cli_data_path(CREDENTIALS_FILENAME)
}
