/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use clap::{Parser, Subcommand};
use merino::curated_recommendations::{
    CuratedRecommendationsClient, CuratedRecommendationsConfig, CuratedRecommendationsRequest,
};

#[derive(Debug, Parser)]
struct Cli {
    /// Optional base host (defaults to prod)
    #[arg(long)]
    base_host: Option<String>,

    /// Required user agent header
    #[arg(long)]
    user_agent: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Query {
        /// JSON string of type CuratedRecommendationsRequest
        #[clap(long)]
        json: Option<String>,

        /// Path to a JSON file containing the request
        #[clap(long, value_name = "FILE")]
        json_file: Option<std::path::PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    viaduct_reqwest::use_reqwest_backend();
    let config = CuratedRecommendationsConfig {
        base_host: cli.base_host.clone(),
        user_agent_header: cli.user_agent.clone(),
    };

    let client = CuratedRecommendationsClient::new(config)?;

    match cli.command {
        Commands::Query { json, json_file } => {
            let json_data = match (json_file, json) {
                (Some(path), _) => std::fs::read_to_string(path)?,
                (None, Some(raw)) => raw,
                (None, None) => anyhow::bail!("You must provide either --json or --json-file"),
            };

            query_from_json(json_data, &client)?;
        }
    };

    Ok(())
}

fn query_from_json(json: String, client: &CuratedRecommendationsClient) -> Result<()> {
    let request: CuratedRecommendationsRequest = serde_json::from_str(&json)?;
    let response = client.get_curated_recommendations(&request)?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
