// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[allow(unused_imports)] // may be unused in some features.
use nimbus::error::{info, Result};

#[cfg(feature = "stateful")]
fn main() -> Result<()> {
    const DEFAULT_BASE_URL: &str = "https://firefox.settings.services.mozilla.com";
    const DEFAULT_COLLECTION_NAME: &str = "messaging-experiments";

    use clap::{App, Arg, SubCommand};
    use nimbus::{
        metrics::{
            EnrollmentStatusExtraDef, FeatureExposureExtraDef, MalformedFeatureConfigExtraDef,
            MetricsHandler,
        },
        AppContext, AvailableRandomizationUnits, EnrollmentStatus, NimbusClient,
        NimbusTargetingHelper, RemoteSettingsConfig, RemoteSettingsServer,
    };
    use std::collections::HashMap;
    use std::io::prelude::*;

    pub struct NoopMetricsHandler;

    impl MetricsHandler for NoopMetricsHandler {
        fn record_enrollment_statuses(&self, _: Vec<EnrollmentStatusExtraDef>) {
            // do nothing
        }

        fn record_feature_activation(&self, _activation_event: FeatureExposureExtraDef) {
            // do nothing
        }

        fn record_feature_exposure(&self, _exposure_event: FeatureExposureExtraDef) {
            // do nothing
        }

        fn record_malformed_feature_config(&self, _event: MalformedFeatureConfigExtraDef) {
            // do nothing
        }
    }

    // We set the logging level to be `warn` here, meaning that only
    // logs of `warn` or higher will be actually be shown, any other
    // error will be omitted
    // To manually set the log level, you can set the `RUST_LOG` environment variable
    // Possible values are "info", "debug", "warn" and "error"
    // Check [`env_logger`](https://docs.rs/env_logger/) for more details
    error_support::init_for_tests_with_level(error_support::Level::Info);
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
            SubCommand::with_name("fetch-experiments")
            .about("Fetch experiments from the server. Subsequent calls to apply-pending-experiments will change enrolments."),
        )
        .subcommand(
            SubCommand::with_name("apply-pending-experiments")
            .about("Updates enrollments with the experiments last fetched from the server with fetch-experiments"),
        )
        .subcommand(
            SubCommand::with_name("update-experiments")
            .about("Equivalent to fetch-experiments and apply-pending-experiments together"),
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
    info!("Server url is {}", server_url);

    let client_id = config
        .get("client_id")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "no-client-id-specified".to_string());
    info!("Client ID is {}", client_id);

    let collection_name =
        matches
            .value_of("collection")
            .unwrap_or_else(|| match config.get("collection_name") {
                Some(v) => v.as_str().unwrap(),
                _ => DEFAULT_COLLECTION_NAME,
            });
    info!("Collection name is {}", collection_name);

    let temp_dir = std::env::temp_dir();
    let db_path_default = temp_dir.to_str().unwrap();
    let db_path = matches
        .value_of("db-path")
        .unwrap_or_else(|| match config.get("db_path") {
            Some(v) => v.as_str().unwrap(),
            _ => db_path_default,
        });
    info!("Database directory is {}", db_path);

    // initiate the optional config
    let config = RemoteSettingsConfig {
        server: Some(RemoteSettingsServer::Custom {
            url: server_url.to_string(),
        }),
        server_url: None,
        bucket_name: None,
        collection_name: collection_name.to_string(),
    };

    // Here we initialize our main `NimbusClient` struct
    let nimbus_client = NimbusClient::new(
        context.clone(),
        Default::default(),
        Default::default(),
        db_path,
        Some(config),
        Box::new(NoopMetricsHandler),
        None,
    )?;
    info!("Nimbus ID is {}", nimbus_client.nimbus_id()?);

    // Explicitly update experiments at least once for init purposes
    nimbus_client.fetch_experiments()?;
    nimbus_client.apply_pending_experiments()?;

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
        ("fetch-experiments", _) => {
            println!("======================================");
            println!("Fetching experiments");
            nimbus_client.fetch_experiments()?;
        }
        ("apply-pending-experiments", _) => {
            println!("======================================");
            println!("Applying pending experiments");
            nimbus_client.apply_pending_experiments()?;
        }
        ("update-experiments", _) => {
            println!("======================================");
            println!("Fetching and applying experiments");
            nimbus_client.fetch_experiments()?;
            nimbus_client.apply_pending_experiments()?;
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
        ("opt-out-all", _) => {
            println!("======================================");
            println!("Opting out of ALL experiments:");
            let experiments = nimbus_client.get_all_experiments().unwrap();
            for experiment in experiments {
                println!("\t'{}'", &experiment.slug);
                nimbus_client.opt_out(experiment.slug)?;
            }
        }
        // gen_uuid will generate a UUID that gets enrolled in a given number of
        // experiments, optionally setting the generated ID in the database.
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
            let aru = AvailableRandomizationUnits::default();
            'outer: loop {
                let uuid = uuid::Uuid::new_v4();
                let aru = aru.apply_nimbus_id(&uuid);
                let mut num_of_experiments_enrolled = 0;
                let event_store = nimbus_client.event_store();
                let th = NimbusTargetingHelper::new(&context, event_store.clone(), None);
                for exp in &all_experiments {
                    let enr = nimbus::evaluate_enrollment(&aru, exp, &th)?;
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
            let event_store = nimbus_client.event_store();
            for _i in 0..num {
                // Rather than inspecting what randomization unit is specified
                // by the experiment just generate a new uuid for all possible
                // options.
                let uuid = uuid::Uuid::new_v4();
                let aru = AvailableRandomizationUnits::with_nimbus_id(&uuid);
                let th = NimbusTargetingHelper::new(&context, event_store.clone(), None);
                let enrollment = nimbus::evaluate_enrollment(&aru, &exp, &th)?;
                let key = match enrollment.status.clone() {
                    EnrollmentStatus::Enrolled { .. } => "Enrolled",
                    EnrollmentStatus::NotEnrolled { .. } => "NotEnrolled",
                    EnrollmentStatus::Disqualified { .. } => "Disqualified",
                    EnrollmentStatus::WasEnrolled { .. } => "WasEnrolled",
                    EnrollmentStatus::Error { .. } => "Error",
                };
                results.insert(key, results.get(&key).unwrap_or(&0) + 1);
            }
            println!("Results: {:#?}", results);
        }
        (&_, _) => println!("Invalid subcommand"),
    };
    Ok(())
}

#[cfg(not(feature = "stateful"))]
fn main() -> Result<()> {
    Ok(())
}
