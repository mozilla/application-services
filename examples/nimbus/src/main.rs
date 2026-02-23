// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use nimbus::{
    error::{info, Result},
    stateful::client::NimbusServerSettings,
};

#[derive(Parser)]
#[command(name = "Nimbus SDK Demo")]
#[command(author = "Tarik E. <teshaq@mozilla.com>")]
#[command(about = "A demo for the Nimbus SDK")]
struct Args {
    /// Custom File configuration
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,

    /// Custom collection name
    #[arg(short = 'n', long)]
    collection: Option<String>,

    #[arg(short, long, value_name = "SERVER_URL")]
    /// Specifies the server to use
    server: Option<String>,

    #[arg(long, value_name = "PATH")]
    /// Path where the database will be created"
    db_path: Option<String>,

    #[command(subcommand)]
    subcommand: Subcommands,
}

#[derive(Subcommand)]
enum Subcommands {
    /// Show all experiments, followed by the enrolled experiments
    ShowExperiments,

    /// Fetch experiments from the server. Subsequent calls to apply-pending-experiments will change enrolments.
    FetchExperiments,

    /// Updates enrollments with the experiments last fetched from the server with fetch-experiments
    ApplyPendingExperiments,

    /// Equivalent to fetch-experiments and apply-pending-experiments together
    UpdateExperiments,

    /// Opts in to an experiment and branch
    OptIn {
        #[arg(long, value_name = "EXPERIMENT_ID")]
        /// The ID of the experiment to opt in to
        experiment: String,

        #[arg(long, value_name = "BRANCH_ID")]
        /// The ID of the branch to opt in to
        branch: String,
    },

    /// Opts out of an experiment
    OptOut {
        #[arg(long, value_name = "EXPERIMENT_ID")]
        /// The ID of the experiment to opt out of
        experiment: String,
    },

    /// Opts out of all experiments
    OptOutAll,

    /// Generate a uuid that can get enrolled in experiments
    GenUuid {
        /// The number of experiments the uuid generated should be able to enroll in,
        /// WARNING: This can end in an infinite loop if the number is too high
        #[arg(long, default_value_t = 1)]
        number: usize,

        /// Sets the UUID in the database when complete.
        #[arg(long)]
        set: bool,
    },

    /// Brute-force an experiment a number of times, showing enrollment results
    BruteForce {
        #[arg(long, value_name = "EXPERIMENT_ID")]
        /// The ID of the experiment to reset
        experiment: String,

        #[arg(short, long, default_value_t = 10000)]
        /// The number of times to generate a UUID and attempt enrollment.
        num: usize,
    },
}

fn main() -> Result<()> {
    const DEFAULT_BASE_URL: &str = "https://firefox.settings.services.mozilla.com";
    const DEFAULT_COLLECTION_NAME: &str = "messaging-experiments";

    use nimbus::{
        metrics::{
            EnrollmentStatusExtraDef, FeatureExposureExtraDef, MalformedFeatureConfigExtraDef,
            MetricsHandler,
        },
        AppContext, AvailableRandomizationUnits, EnrollmentStatus, NimbusClient,
        NimbusTargetingHelper,
    };
    use remote_settings::{RemoteSettingsConfig2, RemoteSettingsService};
    use std::io::prelude::*;
    use std::{collections::HashMap, sync::Arc};

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

        fn submit_targeting_context(&self) {
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
    viaduct_hyper::viaduct_init_backend_hyper().expect("Error initalizing viaduct");

    // Initiate the matches for the command line arguments
    let args = Args::parse();

    // Read command line arguments, or set default values
    let mut config_file = std::fs::File::open(args.config).expect("Config file does not exist");
    let mut config = String::new();
    config_file.read_to_string(&mut config).unwrap();
    let config = serde_json::from_str::<serde_json::Value>(&config).unwrap();

    let context = config.get("context").unwrap();
    let context = serde_json::from_value::<AppContext>(context.clone()).unwrap();
    let server_url = args
        .server
        .as_deref()
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
        args.collection
            .as_deref()
            .unwrap_or_else(|| match config.get("collection_name") {
                Some(v) => v.as_str().unwrap(),
                _ => DEFAULT_COLLECTION_NAME,
            });
    info!("Collection name is {}", collection_name);

    let temp_dir = std::env::temp_dir();
    let db_path_default = temp_dir.to_str().unwrap();
    let db_path = args
        .db_path
        .as_deref()
        .unwrap_or_else(|| match config.get("db_path") {
            Some(v) => v.as_str().unwrap(),
            _ => db_path_default,
        });
    info!("Database directory is {}", db_path);

    // initiate the optional config
    let config = RemoteSettingsConfig2 {
        server: None,
        bucket_name: None,
        app_context: None,
    };

    let remote_settings_services = RemoteSettingsService::new("nimbus".to_owned(), config);

    // Here we initialize our main `NimbusClient` struct
    let nimbus_client = NimbusClient::new(
        context.clone(),
        Default::default(),
        Default::default(),
        db_path,
        Arc::new(NoopMetricsHandler),
        None,
        Some(NimbusServerSettings {
            rs_service: Arc::new(remote_settings_services),
            collection_name: collection_name.to_string(),
        }),
    )?;
    info!("Nimbus ID is {}", nimbus_client.nimbus_id()?);

    // Explicitly update experiments at least once for init purposes
    nimbus_client.fetch_experiments()?;
    nimbus_client.apply_pending_experiments()?;

    // We match against the subcommands
    match args.subcommand {
        // show_enrolled shows only the enrolled experiments and the chosen branches
        Subcommands::ShowExperiments => {
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
        Subcommands::FetchExperiments => {
            println!("======================================");
            println!("Fetching experiments");
            nimbus_client.fetch_experiments()?;
        }
        Subcommands::ApplyPendingExperiments => {
            println!("======================================");
            println!("Applying pending experiments");
            nimbus_client.apply_pending_experiments()?;
        }
        Subcommands::UpdateExperiments => {
            println!("======================================");
            println!("Fetching and applying experiments");
            nimbus_client.fetch_experiments()?;
            nimbus_client.apply_pending_experiments()?;
        }
        Subcommands::OptIn { experiment, branch } => {
            println!("======================================");
            println!(
                "Opting in to experiment '{}', branch '{}'",
                experiment, branch
            );
            nimbus_client.opt_in_with_branch(experiment.to_string(), branch.to_string())?;
        }
        Subcommands::OptOut { experiment } => {
            println!("======================================");
            println!("Opting out of experiment '{}'", experiment);
            nimbus_client.opt_out(experiment.to_string())?;
        }
        Subcommands::OptOutAll => {
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
        Subcommands::GenUuid { number, set } => {
            let all_experiments = nimbus_client.get_all_experiments()?;
            // XXX - this check below isn't good enough - we need to know how
            // many of those experiments we are actually eligible for!
            if all_experiments.len() < number {
                println!(
                    "Can't try to enroll in {} experiments - only {} exist",
                    number,
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
                        if num_of_experiments_enrolled >= number {
                            println!("======================================");
                            println!("Generated UUID is: {}", uuid);
                            println!("(it took {} goes to find it)", num_tries);
                            // ideally we'd
                            if set {
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
        Subcommands::BruteForce {
            experiment: experiment_id,
            num,
        } => {
            println!("Brute-forcing experiment '{}' {} times", experiment_id, num);

            // *sob* no way currently to get by id.
            let find_exp = || {
                for exp in nimbus_client
                    .get_all_experiments()
                    .expect("can't fetch experiments!?")
                {
                    if exp.slug == *experiment_id {
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
    };
    Ok(())
}
