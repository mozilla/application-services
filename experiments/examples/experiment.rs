// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use clap::{App, Arg, SubCommand};
use env_logger::Env;
use experiments::{AppContext, ExperimentConfig, Experiments};
use std::io::prelude::*;

const DEFAULT_BASE_URL: &str = "https://settings.stage.mozaws.net"; // TODO: Replace this with prod
const DEFAULT_BUCKET_NAME: &str = "main";
const DEFAULT_COLLECTION_NAME: &str = "messaging-experiments";
fn main() {
    // We set the logging level to be `warn` here, meaning that only
    // logs of `warn` or higher will be actually be shown, any other
    // error will be omitted
    // To manually set the log level, you can set the `RUST_LOG` environment variable
    // Possible values are "info", "debug", "warn" and "error"
    // Check [`env_logger`](https://docs.rs/env_logger/) for more details
    env_logger::from_env(Env::default().default_filter_or("info")).init();
    viaduct_reqwest::use_reqwest_backend();

    // Initiate the matches for the command line arguments
    let matches = App::new("Nimbus SDK")
        .author("Tarik E. <teshaq@mozilla.com>")
        .about("A demo for the Nimbus SDK")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom File configuration")
                .required(true)
                .takes_value(true),
        )
        .subcommand(
            SubCommand::with_name("show_experiments")
                .about("Show all experiments, followed by the enrolled experiments"),
        )
        .subcommand(
            SubCommand::with_name("gen_uuid")
            .about("Generate a uuid that can get enrolled in experiments")
            .arg(
                Arg::with_name("number")
                .help("The number of experiments the uuid generated should be able to enroll in, WARNING: This can end in an infinite loop if the number is too high")
            ))
        .get_matches();

    // Read command line arguments, or set default values
    let config_file = matches
        .value_of("config")
        .unwrap_or("./examples/context.json");
    let mut config_file = std::fs::File::open(config_file).unwrap();
    let mut config = String::new();
    config_file.read_to_string(&mut config).unwrap();
    let config = serde_json::from_str::<serde_json::Value>(&config).unwrap();

    let context = config.get("context").unwrap();
    let context = serde_json::from_value::<AppContext>(context.clone()).unwrap();
    let server_url = match config.get("server_url") {
        Some(v) => v.as_str().unwrap(),
        _ => DEFAULT_BASE_URL,
    };
    log::info!("Server url is {}", server_url);
    let bucket_name = match config.get("bucket_name") {
        Some(v) => v.as_str().unwrap(),
        _ => DEFAULT_BUCKET_NAME,
    };
    log::info!("Bucket name is {}", bucket_name);
    let collection_name = match config.get("collection_name") {
        Some(v) => v.as_str().unwrap(),
        _ => DEFAULT_COLLECTION_NAME,
    };

    log::info!("Collection name is {}", collection_name);

    // For the uuid, we do not set a default value, instead
    // a random uuid will be generated if none is provided
    let uuid = config.get("uuid").map(|u| u.as_str().unwrap().to_string());

    // initiate the optional config
    let config = ExperimentConfig {
        server_url: Some(server_url.to_string()),
        bucket_name: Some(bucket_name.to_string()),
        uuid,
    };

    // Here we initialize our main `Experiments` struct
    let experiments =
        Experiments::new(collection_name.to_string(), context, "", Some(config)).unwrap();

    // We match against the subcommands
    match matches.subcommand() {
        // show_enrolled shows only the enrolled experiments and the chosen branches
        ("show_experiments", _) => {
            println!("======================================");
            println!("Printing all experiments (regardless of enrollment)");
            experiments
                .get_all_experiments()
                .iter()
                .for_each(|e| println!("Experiment: {}", e.id));
            println!("======================================");
            println!("Printing only enrolled experiments");
            experiments.get_active_experiments().iter().for_each(|e| {
                println!(
                    "Enrolled in experiment: {}, in branch: {}",
                    e.slug, e.branch_slug
                )
            });
        }
        // gen_uuid will generate a UUID that gets enrolled in a given number of
        // experiments
        ("gen_uuid", Some(matches)) => {
            let num = matches
                .value_of("number")
                .unwrap_or("0")
                .parse::<usize>()
                .expect("the number parameter should be a number");
            let all_experiments = experiments.get_all_experiments();
            let mut num_of_experiments_enrolled = 0;
            let mut uuid = uuid::Uuid::new_v4();
            while num_of_experiments_enrolled != num {
                uuid = uuid::Uuid::new_v4();
                num_of_experiments_enrolled = experiments::filter_enrolled(&uuid, &all_experiments)
                    .unwrap()
                    .len()
            }
            println!("======================================");
            println!("Generated Uuid is: {}", uuid);
        }
        (&_, _) => println!("Invalid subcommand"),
    }
}
