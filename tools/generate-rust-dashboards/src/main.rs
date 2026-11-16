/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fs;

use anyhow::{anyhow, Result};
use camino::Utf8PathBuf;
use clap::Parser;

pub mod config;
pub mod metrics;
pub mod schema;
mod sql;
mod teams;
pub mod util;

#[derive(Parser, Debug)]
#[clap(name = "generate-rust-dashboard")]
struct Cli {
    /// Your team name
    team_name: String,

    /// Directory to write JSON files to
    output_dir: Utf8PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dashboards = teams::all_dashboards()?
        .into_iter()
        .find(|d| {
            d.team_name
                .to_ascii_lowercase()
                .contains(&cli.team_name.to_ascii_lowercase())
        })
        .ok_or_else(|| anyhow!("Dashboard not found: {}", cli.team_name))?;

    if !cli.output_dir.exists() {
        fs::create_dir_all(&cli.output_dir)?;
    }

    println!();
    println!("Generating Dashboards:");
    let dashboards = std::iter::once(dashboards.main).chain(dashboards.extra);
    for dashboard in dashboards {
        let mut value = serde_json::to_value(&dashboard)?;
        value.sort_all_objects();
        let content = serde_json::to_string_pretty(&value)?;
        let path = cli.output_dir.join(format!("{}.json", dashboard.uid));
        fs::write(&path, content)?;
        println!("{path}");
    }

    println!();
    println!("* Go to https://yardstick.mozilla.org/dashboards");
    println!("* Create/navigate to a folder for your team");
    println!("* Click New -> Import and import each generated JSON file");

    Ok(())
}
