// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use clap::{App, Arg, SubCommand};
use env_logger::Env;
use nimbus::{
    error::Result, AppContext, AvailableRandomizationUnits, NimbusClient, RemoteSettingsConfig,
};
use std::collections::HashMap;
use std::io::prelude::*;

const DEFAULT_BASE_URL: &str = "https://settings.stage.mozaws.net"; // TODO: Replace this with prod
const DEFAULT_BUCKET_NAME: &str = "main";
const DEFAULT_COLLECTION_NAME: &str = "messaging-experiments";
fn main() -> Result<()> {
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
        .arg(
            Arg::with_name("collection")
                .short("n")
                .long("collection")
                .value_name("COLLECTION")
                .help("Sets a custom collection name")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("server")
                .short("s")
                .long("server")
                .value_name("SERVER_URL")
                .help("Specifies the server to use")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("db-path")
                .long("db-path")
                .value_name("PATH")
                .help("The path where the database will be created")
                .takes_value(true),
        )
        .subcommand(
            SubCommand::with_name("show-experiments")
                .about("Show all experiments, followed by the enrolled experiments"),
        )
        .subcommand(
            SubCommand::with_name("update-experiments")
            .about("Updates experiments and enrollments from the server"),
        )
        .subcommand(
            SubCommand::with_name("opt-in")
            .about("Opts in to an experiment and branch")
            .arg(
                Arg::with_name("experiment")
                .long("experiment")
                .value_name("EXPERIMENT_ID")
                .help("The ID of the experiment to opt in to")
                .required(true)
                .takes_value(true)
            )
            .arg(
                Arg::with_name("branch")
                .long("branch")
                .value_name("BRANCH_ID")
                .help("The ID of the branch to opt in to")
                .required(true)
                .takes_value(true)
            )
        )
        .subcommand(
            SubCommand::with_name("opt-out")
            .about("Opts out of an experiment")
            .arg(
                Arg::with_name("experiment")
                .long("experiment")
                .value_name("EXPERIMENT_ID")
                .help("The ID of the experiment to opt out of")
                .required(true)
                .takes_value(true)
            )
        )
        .subcommand(
            SubCommand::with_name("opt-out-all")
            .about("Opts out of all experiments")
        )
        .subcommand(
            SubCommand::with_name("reset-enrollment")
            .about("Resets enrollment information for the specified experiment")
            .arg(
                Arg::with_name("experiment")
                .long("experiment")
                .value_name("EXPERIMENT_ID")
                .help("The ID of the experiment to reset")
                .required(true)
                .takes_value(true)
            )
        )
        .subcommand(
            SubCommand::with_name("gen-uuid")
            .about("Generate a uuid that can get enrolled in experiments")
            .arg(
                Arg::with_name("number")
                .default_value("1")
                .help("The number of experiments the uuid generated should be able to enroll in, WARNING: This can end in an infinite loop if the number is too high")
            )
            .arg(
                Arg::with_name("set")
                .long("set")
                .help("Sets the UUID in the database when complete.")
            )
        )
        .subcommand(
            SubCommand::with_name("brute-force")
            .about("Brute-force an experiment a number of times, showing enrollment results")
            .arg(
                Arg::with_name("experiment")
                .long("experiment")
                .value_name("EXPERIMENT_ID")
                .help("The ID of the experiment to reset")
                .required(true)
                .takes_value(true)
            )
            .arg(
                Arg::with_name("num")
                .long("num")
                .short("n")
                .default_value("10000")
                .help("The number of times to generate a UUID and attempt enrollment.")
            )
        )
        .get_matches();

    // Read command line arguments, or set default values
    let mut config_file = std::fs::File::open(matches.value_of("config").unwrap())
        .expect("Config file does not exist");
    let mut config = String::new();
    config_file.read_to_string(&mut config).unwrap();
    let config = serde_json::from_str::<serde_json::Value>(&config).unwrap();

    let context = config.get("context").unwrap();
    let context = serde_json::from_value::<AppContext>(context.clone()).unwrap();
    let server_url = matches
        .value_of("server")
        .unwrap_or_else(|| match config.get("server_url") {
            Some(v) => v.as_str().unwrap(),
            _ => DEFAULT_BASE_URL,
        });
    log::info!("Server url is {}", server_url);

    let client_id = config
        .get("client_id")
        .map(|v| v.to_string())
        .unwrap_or("no-client-id-specified".to_string());
    log::info!("Client ID is {}", client_id);

    let bucket_name = match config.get("bucket_name") {
        Some(v) => v.as_str().unwrap(),
        _ => DEFAULT_BUCKET_NAME,
    };
    log::info!("Bucket name is {}", bucket_name);

    let collection_name =
        matches
            .value_of("collection")
            .unwrap_or_else(|| match config.get("collection_name") {
                Some(v) => v.as_str().unwrap(),
                _ => DEFAULT_COLLECTION_NAME,
            });
    log::info!("Collection name is {}", collection_name);

    let temp_dir = std::env::temp_dir();
    let db_path_default = temp_dir.to_str().unwrap();
    let db_path = matches
        .value_of("db-path")
        .unwrap_or_else(|| match config.get("db_path") {
            Some(v) => v.as_str().unwrap(),
            _ => &db_path_default,
        });
    log::info!("Database directory is {}", db_path);

    // initiate the optional config
    let config = RemoteSettingsConfig {
        server_url: server_url.to_string(),
        collection_name: collection_name.to_string(),
        bucket_name: bucket_name.to_string(),
    };

    let aru = AvailableRandomizationUnits {
        client_id: client_id.clone(),
    };

    // Here we initialize our main `NimbusClient` struct
    let nimbus_client = NimbusClient::new(context, "", config, aru)?;

    // We match against the subcommands
    match matches.subcommand() {
        // show_enrolled shows only the enrolled experiments and the chosen branches
        ("show-experiments", _) => {
            println!("======================================");
            println!("Printing all experiments (regardless of enrollment)");
            nimbus_client
                .get_all_experiments()?
                .iter()
                .for_each(|e| println!("Experiment: {}", e.slug));
            println!("======================================");
            println!("Printing only enrolled experiments");
            nimbus_client
                .get_active_experiments()?
                .iter()
                .for_each(|e| {
                    println!(
                        "Enrolled in experiment: {}, in branch: {}",
                        e.slug, e.branch_slug
                    )
                });
        }
        ("update-experiments", _) => {
            println!("======================================");
            println!("Updating experiments");
            nimbus_client.update_experiments()?;
        }
        ("opt-in", Some(matches)) => {
            println!("======================================");
            let experiment = matches.value_of("experiment").unwrap();
            let branch = matches.value_of("branch").unwrap();
            println!(
                "Opting in to experiment '{}', branch '{}'",
                experiment, branch
            );
            nimbus_client.opt_in_with_branch(experiment.to_string(), branch.to_string())?;
        }
        ("opt-out", Some(matches)) => {
            println!("======================================");
            let experiment = matches.value_of("experiment").unwrap();
            println!("Opting out of experiment '{}'", experiment);
            nimbus_client.opt_out(experiment.to_string())?;
        }
        ("reset-enrollment", Some(matches)) => {
            println!("======================================");
            let experiment = matches.value_of("experiment").unwrap();
            println!("Resetting enrollment of experiment '{}'", experiment);
            nimbus_client.reset_enrollment(experiment.to_string())?;
        }
        // gen_uuid will generate a UUID that gets enrolled in a given number of
        // experiments, optionally settting the generated ID in the database.
        ("gen-uuid", Some(matches)) => {
            let num = matches
                .value_of("number")
                .unwrap()
                .parse::<usize>()
                .expect("the number parameter should be a number");
            let all_experiments = nimbus_client.get_all_experiments()?;
            // XXX - this check below isn't good enough - we need to know how
            // many of those experiments we are actually eligible for!
            if all_experiments.len() < num {
                println!(
                    "Can't try to enroll in {} experiments - only {} exist",
                    num,
                    all_experiments.len(),
                );
                std::process::exit(1);
            }

            let mut num_tries = 0;
            let aru = AvailableRandomizationUnits { client_id };
            'outer: loop {
                let uuid = uuid::Uuid::new_v4();
                let mut num_of_experiments_enrolled = 0;
                for exp in &all_experiments {
                    let enr = nimbus::evaluate_enrollment(&uuid, &aru, &Default::default(), &exp)?;
                    if enr.status.is_enrolled() {
                        num_of_experiments_enrolled += 1;
                        if num_of_experiments_enrolled >= num {
                            println!("======================================");
                            println!("Generated UUID is: {}", uuid);
                            println!("(it took {} goes to find it)", num_tries);
                            // ideally we'd
                            if matches.is_present("set") {
                                println!("Setting uuid in the database...");
                                nimbus_client.set_nimbus_id(&uuid)?;
                            }
                            break 'outer;
                        }
                    }
                }
                num_tries += 1;
                if num_tries % 5000 == 0 {
                    println!(
                        "Made {} attempts so far; it's not looking good...",
                        num_tries
                    );
                }
            }
        }
        ("brute-force", Some(matches)) => {
            let experiment_id = matches.value_of("experiment").unwrap();
            let num = matches
                .value_of("num")
                .unwrap()
                .parse::<usize>()
                .expect("the number of iterations to brute-force");
            println!("Brute-forcing experiment '{}' {} times", experiment_id, num);

            // *sob* no way currently to get by id.
            let find_exp = || {
                for exp in nimbus_client
                    .get_all_experiments()
                    .expect("can't fetch experiments!?")
                {
                    if exp.slug == experiment_id {
                        return exp;
                    }
                }
                panic!("No such experiment");
            };
            let exp = find_exp();
            let mut results = HashMap::new();
            for _i in 0..num {
                // Rather than inspecting what randomization unit is specified
                // by the experiment just generate a new uuid for all possible
                // options.
                let uuid = uuid::Uuid::new_v4();
                let aru = AvailableRandomizationUnits {
                    client_id: uuid.to_string(),
                };
                let enrollment =
                    nimbus::evaluate_enrollment(&uuid, &aru, &Default::default(), &exp)?;
                results.insert(
                    enrollment.status.clone(),
                    results.get(&enrollment.status).unwrap_or(&0) + 1,
                );
            }
            println!("Results: {:#?}", results);
        }
        (&_, _) => println!("Invalid subcommand"),
    };
    Ok(())
}
