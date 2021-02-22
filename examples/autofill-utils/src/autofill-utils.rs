/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use anyhow::Result;
use autofill::db::{
    models::{address, credit_card},
    store::Store,
};
use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config};
use interrupt_support::NeverInterrupts;
use std::{fs::File, io::BufReader};
use structopt::StructOpt;
use sync15::{
    sync_multiple, EngineSyncAssociation, MemoryCachedState, SetupStorageClient,
    Sync15StorageClient, SyncEngine,
}; // XXX need a real interruptee!

// Note: this uses doc comments to generate the help text.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "autofill-utils", about = "Command-line utilities for autofill")]
pub struct Opts {
    /// Sets the path to the database
    #[structopt(
        name = "database_path",
        long,
        short = "d",
        default_value = "./autofill.db"
    )]
    pub database_path: String,

    /// Disables all logging (useful for performance evaluation)
    #[structopt(name = "no-logging", long)]
    pub no_logging: bool,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, StructOpt)]
enum Command {
    /// Adds JSON address
    #[structopt(name = "add-address")]
    AddAddress {
        #[structopt(name = "input-file", long, short = "i")]
        /// The input file containing the address to be added
        input_file: String,
    },

    /// Gets address from database
    #[structopt(name = "get-address")]
    GetAddress {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the address to retrieve
        guid: String,
    },

    /// Gets all addresses from database
    #[structopt(name = "get-all-addresses")]
    GetAllAddresses,

    /// Update address with given JSON address data
    #[structopt(name = "update-address")]
    UpdateAddress {
        #[structopt(name = "guid", long)]
        /// The guid of the item to update
        guid: String,
        #[structopt(name = "input-file", long, short = "i")]
        /// The input file containing the address data
        input_file: String,
    },

    /// Delete address from database
    #[structopt(name = "delete-address")]
    DeleteAddress {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the address to delete
        guid: String,
    },

    /// Adds JSON credit card
    #[structopt(name = "add-credit-card")]
    AddCreditCard {
        #[structopt(name = "input-file", long, short = "i")]
        /// The input file containing the credit card to be added
        input_file: String,
    },

    /// Gets credit card from database
    #[structopt(name = "get-credit-card")]
    GetCreditCard {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the credit card to retrieve
        guid: String,
    },

    /// Gets all credit cards from database
    #[structopt(name = "get-all-credit-cards")]
    GetAllCreditCards,

    /// Update credit card with given JSON credit card data
    #[structopt(name = "update-credit-card")]
    UpdateCreditCard {
        #[structopt(name = "guid", long)]
        /// The guid of the item to update
        guid: String,
        #[structopt(name = "input-file", long, short = "i")]
        /// The input file containing the credit card data
        input_file: String,
    },

    /// Delete credit card from database
    #[structopt(name = "delete-credit-card")]
    DeleteCreditCard {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the credit card to delete
        guid: String,
    },

    /// Syncs all or some engines.
    Sync {
        #[structopt(name = "engines", long)]
        /// Path to store our cached fxa credentials.
        #[structopt(name = "credentials", long, default_value = "./credentials.json")]
        credential_file: String,

        /// Wipe ALL storage from the server before syncing.
        #[structopt(name = "wipe-all-remote", long)]
        wipe_all: bool,

        /// Wipe the engine data from the server before syncing.
        #[structopt(name = "wipe-remote", long)]
        wipe: bool,

        /// Reset the engine before syncing
        #[structopt(name = "reset", long)]
        reset: bool,

        /// Number of syncs to perform
        #[structopt(name = "nsyncs", long, default_value = "1")]
        nsyncs: u32,

        /// Number of milliseconds to wait between syncs
        #[structopt(name = "wait", long, default_value = "0")]
        wait: u64,
    },
}

fn run_add_address(store: &Store, filename: String) -> Result<()> {
    println!("Retrieving address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: address::UpdatableAddressFields = serde_json::from_reader(reader)?;

    println!("Making `add_address` api call");
    let address = Store::add_address(store, address_fields)?;

    println!("Created address: {:#?}", address);
    Ok(())
}

fn run_get_address(store: &Store, guid: String) -> Result<()> {
    println!("Getting address for guid `{}`", guid);

    let address = Store::get_address(store, guid)?;

    println!("Retrieved address: {:#?}", address);
    Ok(())
}

fn run_get_all_addresses(store: &Store) -> Result<()> {
    println!("Getting all addresses");

    let addresses = Store::get_all_addresses(store)?;

    println!("Retrieved addresses: {:#?}", addresses);

    Ok(())
}

fn run_update_address(store: &Store, guid: String, filename: String) -> Result<()> {
    println!("Updating address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: address::UpdatableAddressFields = serde_json::from_reader(reader)?;

    println!("Making `update_address` api call for guid {}", guid);
    Store::update_address(store, guid.clone(), address_fields)?;

    let address = Store::get_address(store, guid)?;
    println!("Updated address: {:#?}", address);

    Ok(())
}

fn run_delete_address(store: &Store, guid: String) -> Result<()> {
    println!("Deleting address for guid `{}`", guid);

    Store::delete_address(store, guid)?;

    println!("Successfully deleted address");
    Ok(())
}

fn run_add_credit_card(store: &Store, filename: String) -> Result<()> {
    println!("Retrieving credit card data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let credit_card_fields: credit_card::UpdatableCreditCardFields =
        serde_json::from_reader(reader)?;

    println!("Making `add_credit_card` api call");
    let credit_card = Store::add_credit_card(store, credit_card_fields)?;

    println!("Created credit card: {:#?}", credit_card);
    Ok(())
}

fn run_get_credit_card(store: &Store, guid: String) -> Result<()> {
    println!("Getting credit card for guid `{}`", guid);

    let credit_card = Store::get_credit_card(store, guid)?;

    println!("Retrieved credit card: {:#?}", credit_card);
    Ok(())
}

fn run_get_all_credit_cards(store: &Store) -> Result<()> {
    println!("Getting all credit cards");

    let credit_cards = Store::get_all_credit_cards(store)?;

    println!("Retrieved credit cards: {:#?}", credit_cards);

    Ok(())
}

fn run_update_credit_card(store: &Store, guid: String, filename: String) -> Result<()> {
    println!("Updating credit card data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let credit_card_fields: credit_card::UpdatableCreditCardFields =
        serde_json::from_reader(reader)?;

    println!("Making `update_credit_card` api call for guid {}", guid);
    Store::update_credit_card(store, guid.clone(), credit_card_fields)?;

    let credit_card = Store::get_credit_card(store, guid)?;
    println!("Updated credit card: {:#?}", credit_card);

    Ok(())
}

fn run_delete_credit_card(store: &Store, guid: String) -> Result<()> {
    println!("Deleting credit card for guid `{}`", guid);

    Store::delete_credit_card(store, guid)?;

    println!("Successfully deleted credit card");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_sync(
    store: &Store,
    cred_file: String,
    wipe_all: bool,
    wipe: bool,
    reset: bool,
    nsyncs: u32,
    wait: u64,
) -> Result<()> {
    // XXX - need to add interrupts
    let cli_fxa = get_cli_fxa(get_default_fxa_config(), &cred_file)?;

    if wipe_all {
        Sync15StorageClient::new(cli_fxa.client_init.clone())?.wipe_all_remote()?;
    }
    let mut mem_cached_state = MemoryCachedState::default();
    let mut global_state: Option<String> = None;
    let engines: Vec<Box<dyn SyncEngine>> = vec![
        store.create_addresses_sync_engine(),
        store.create_credit_cards_sync_engine(),
    ];
    for engine in &engines {
        if wipe {
            engine.wipe()?;
        }
        if reset {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
    }

    // now the syncs.
    // For now we never persist the global state, which means we may lose
    // which engines are declined.
    // That's OK for the short term, and ultimately, syncing functionality
    // will be in places_api, which will give us this for free.
    let mut error_to_report = None;
    let engines_to_sync: Vec<&dyn SyncEngine> = engines.iter().map(AsRef::as_ref).collect();

    for n in 0..nsyncs {
        let mut result = sync_multiple(
            &engines_to_sync,
            &mut global_state,
            &mut mem_cached_state,
            &cli_fxa.client_init.clone(),
            &cli_fxa.root_sync_key,
            &NeverInterrupts,
            None,
        );

        for (name, result) in result.engine_results.drain() {
            match result {
                Ok(()) => log::info!("Status for {:?}: Ok", name),
                Err(e) => {
                    log::warn!("Status for {:?}: {:?}", name, e);
                    error_to_report = Some(e);
                }
            }
        }

        match result.result {
            Err(e) => {
                log::warn!("Sync failed! {}", e);
                log::warn!("BT: {:?}", e.backtrace());
                error_to_report = Some(e);
            }
            Ok(()) => log::info!("Sync was successful!"),
        }

        println!("Sync service status: {:?}", result.service_status);
        println!(
            "Sync telemetry: {}",
            serde_json::to_string_pretty(&result.telemetry).unwrap()
        );

        if n < nsyncs - 1 {
            log::info!("Waiting {}ms before syncing again...", wait);
            std::thread::sleep(std::time::Duration::from_millis(wait));
        }
    }

    // return an error if any engine failed.
    match error_to_report {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

fn main() -> Result<()> {
    viaduct_reqwest::use_reqwest_backend();

    let opts = Opts::from_args();
    if !opts.no_logging {
        cli_support::init_trace_logging();
    }

    let db_path = opts.database_path;
    let store = Store::new(db_path)?;

    match opts.cmd {
        Command::AddAddress { input_file } => run_add_address(&store, input_file),
        Command::GetAddress { guid } => run_get_address(&store, guid),
        Command::GetAllAddresses => run_get_all_addresses(&store),
        Command::UpdateAddress { guid, input_file } => run_update_address(&store, guid, input_file),
        Command::DeleteAddress { guid } => run_delete_address(&store, guid),

        Command::AddCreditCard { input_file } => run_add_credit_card(&store, input_file),
        Command::GetCreditCard { guid } => run_get_credit_card(&store, guid),
        Command::GetAllCreditCards => run_get_all_credit_cards(&store),
        Command::UpdateCreditCard { guid, input_file } => {
            run_update_credit_card(&store, guid, input_file)
        }
        Command::DeleteCreditCard { guid } => run_delete_credit_card(&store, guid),
        Command::Sync {
            credential_file,
            wipe_all,
            wipe,
            reset,
            nsyncs,
            wait,
        } => run_sync(&store, credential_file, wipe_all, wipe, reset, nsyncs, wait),
    }
}
