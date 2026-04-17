/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod devices;
mod send_tab;

use clap::{Parser, Subcommand, ValueEnum};
use cli_support::fxa_creds::{self, CliFxa, WELL_KNOWN_SCOPES};
use fxa_client::{FxaConfig, FxaServer};

static CLIENT_ID: &str = "a2270f727f45f648";

use anyhow::{bail, Result};

#[derive(Parser)]
#[command(about, long_about = None, after_help=format!("Some well known scopes:\n{WELL_KNOWN_SCOPES:#?}"))]
struct Cli {
    /// The FxA server to use
    #[clap(long)]
    server: Option<Server>,

    /// Custom FxA server URL
    #[clap(long)]
    custom_url: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
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
    /// * Start with a fresh account by using `cargo run --example fxa-client disconnect`
    /// * Log in to the account with `cargo run --example fxa-client login`
    /// * Run `RUST_LOG=trace cargo run --example fxa-client fxa get-access-token https://identity.mozilla.com/apps/relay`.
    ///   * You should see a token exchange request in the trace logs.
    /// * Run `RUST_LOG=trace cargo run --example fxa-client https://identity.mozilla.com/apps/relay`
    ///   again.
    ///   * This time there shouldn't be a token exchange request, since the refresh token now has
    ///     the relay scope.
    GetAccessToken {
        scope: String,
    },
    /// Log in to FxA with the given scopes
    Login {
        /// OAuth scopes to request (repeatable), if not supplied, sync, session and profile scopes are requested.
        #[clap(long = "scope", required = true)]
        scopes: Vec<String>,
    },
    Disconnect,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    nss::ensure_initialized();
    viaduct_hyper::viaduct_init_backend_hyper()?;
    cli_support::init_logging_with("info");

    println!();
    let mut fxa = make_cli_fxa(&cli)?;

    match cli.command {
        None => {
            println!("A utility to help manage a Firefox account in a CLI environment");
            println!("The account state managed by this utility can be used by many app-services demos and examples.");
            println!("Run with `help` or `--help` for more");
            print_status(&fxa);
            return Ok(());
        }
        Some(Command::Login { scopes }) => {
            let scope_refs: Vec<&str> = if scopes.is_empty() {
                vec![fxa_creds::SYNC_SCOPE, fxa_creds::SESSION_SCOPE]
            } else {
                scopes.iter().map(|s| s.as_str()).collect()
            };
            fxa.ensure_logged_in(&scope_refs)?;
            print_status(&fxa);
        }
        Some(command) => {
            let Some(account) = fxa.account() else {
                println!("not logged in");
                return Ok(());
            };
            match command {
                Command::Devices(args) => devices::run(account, args)?,
                Command::SendTab(args) => send_tab::run(account, args)?,
                Command::GetAccessToken { scope } => {
                    println!("Requesting access token with scope: {scope}");
                    account.get_access_token(&scope, false)?;
                    println!("Success");
                }
                Command::Disconnect => {
                    account.disconnect();
                }
                Command::Login { .. } => unreachable!(),
            }
        }
    }
    fxa.persist()?;

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

fn print_status(fxa: &CliFxa) {
    match fxa.account() {
        None => println!("Not logged in"),
        Some(account) => match account.check_authorization_status() {
            Ok(status) if status.active => {
                println!("Account is logged in and authorized by the server")
            }
            Ok(_) => println!("Account is logged in but not authorized by the server"),
            Err(e) => println!("Account logged in but account status failed: {e}"),
        },
    }
}

fn make_cli_fxa(cli: &Cli) -> Result<CliFxa> {
    let server = cli.server()?;
    let redirect_uri = format!("{}/oauth/success/a2270f727f45f648", server.content_url());
    let config = FxaConfig {
        server,
        redirect_uri,
        client_id: CLIENT_ID.into(),
        token_server_url_override: None,
    };
    CliFxa::new(config, Some(fxa_creds::CREDENTIALS_FILENAME))
}
