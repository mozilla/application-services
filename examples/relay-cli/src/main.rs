/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{Parser, Subcommand};
use std::io::{self, Write};

use relay::RelayClient;

#[derive(Debug, Parser)]
#[command(name = "relay", about = "CLI tool for interacting with Mozilla Relay", long_about = None)]
struct Cli {
    #[arg(short, long, action)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Fetch,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(&cli);

    viaduct_reqwest::use_reqwest_backend();

    let token = prompt_token()?;
    let client = RelayClient::new("https://relay.firefox.com".to_string(), Some(token));

    match cli.command {
        Commands::Fetch => fetch_addresses(client?),
    }
}

fn init_logging(cli: &Cli) {
    let log_filter = if cli.verbose {
        "relay=trace"
    } else {
        "relay=info"
    };
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", log_filter));
}

fn prompt_token() -> anyhow::Result<String> {
    println!(
        "See https://github.com/mozilla/fx-private-relay/blob/main/docs/api_auth.md#debugging-tip"
    );
    print!("Enter your Relay auth token: ");
    io::stdout().flush()?;
    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    Ok(token.trim().to_string())
}

fn fetch_addresses(client: RelayClient) -> anyhow::Result<()> {
    match client.fetch_addresses() {
        Ok(addresses) => {
            println!("Fetched {} addresses:", addresses.len());
            for address in addresses {
                println!("{:#?}", address);
            }
        }
        Err(e) => {
            eprintln!("Failed to fetch addresses: {:?}", e);
        }
    }
    Ok(())
}
