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
use cli_support::prompt::{prompt_string, prompt_usize};
use interrupt_support::NeverInterrupts;
use structopt::StructOpt;
use sync15::{
    sync_multiple, EngineSyncAssociation, MemoryCachedState, SetupStorageClient,
    Sync15StorageClient, SyncEngine,
}; // XXX need a real interruptee!

fn update_string(field_name: &str, field: String) -> String {
    let opt_s = prompt_string(format!(
        "new {} [now '{}' - leave blank to keep the same value]",
        field_name, field
    ));
    if let Some(s) = opt_s {
        s
    } else {
        field
    }
}

fn update_i64(field_name: &str, field: i64) -> i64 {
    let opt = prompt_usize(format!(
        "new {} [now '{}' - leave blank to keep the same value]",
        field_name, field
    ));
    if let Some(s) = opt {
        s as i64
    } else {
        field
    }
}

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
    AddAddress {},

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
    AddCreditCard {},

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

fn run_add_address(store: &Store) -> Result<()> {
    let address_fields = address::UpdatableAddressFields {
        given_name: prompt_string("given_name").unwrap_or_default(),
        additional_name: prompt_string("additional_name").unwrap_or_default(),
        family_name: prompt_string("family_name").unwrap_or_default(),
        organization: prompt_string("organization").unwrap_or_default(),
        street_address: prompt_string("street_address").unwrap_or_default(),
        address_level3: prompt_string("address_level3").unwrap_or_default(),
        address_level2: prompt_string("address_level2").unwrap_or_default(),
        address_level1: prompt_string("address_level1").unwrap_or_default(),
        postal_code: prompt_string("postal_code").unwrap_or_default(),
        country: prompt_string("country").unwrap_or_default(),
        tel: prompt_string("tel").unwrap_or_default(),
        email: prompt_string("email").unwrap_or_default(),
    };

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

fn run_update_address(store: &Store, guid: String) -> Result<()> {
    let address = Store::get_address(store, guid.clone())?;

    let updatable = address::UpdatableAddressFields {
        given_name: update_string("given_name", address.given_name),
        additional_name: update_string("additional_name", address.additional_name),
        family_name: update_string("family_name", address.family_name),
        organization: update_string("organization", address.organization),
        street_address: update_string("street_address", address.street_address),
        address_level3: update_string("address_level3", address.address_level3),
        address_level2: update_string("address_level2", address.address_level2),
        address_level1: update_string("address_level1", address.address_level1),
        postal_code: update_string("postal_code", address.postal_code),
        country: update_string("country", address.country),
        tel: update_string("tel", address.tel),
        email: update_string("email", address.email),
    };

    println!("Making `update_address` api call for guid {}", guid);
    Store::update_address(store, guid.clone(), updatable)?;

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

fn run_add_credit_card(store: &Store) -> Result<()> {
    let cc_fields = credit_card::UpdatableCreditCardFields {
        cc_name: prompt_string("cc_name").unwrap_or_default(),
        cc_number: prompt_string("cc_number").unwrap_or_default(),
        cc_exp_month: prompt_usize("cc_exp_month").unwrap_or_default() as i64,
        cc_exp_year: prompt_usize("cc_exp_year").unwrap_or_default() as i64,
        cc_type: prompt_string("cc_type").unwrap_or_default(),
    };
    println!("Making `add_credit_card` api call");
    let credit_card = Store::add_credit_card(store, cc_fields)?;

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

fn run_update_credit_card(store: &Store, guid: String) -> Result<()> {
    let cc = Store::get_credit_card(store, guid.clone())?;
    let updatable = credit_card::UpdatableCreditCardFields {
        cc_name: update_string("cc_name", cc.cc_name),
        cc_number: update_string("cc_number", cc.cc_number),
        cc_exp_month: update_i64("cc_exp_month", cc.cc_exp_month),
        cc_exp_year: update_i64("cc_exp_year", cc.cc_exp_year),
        cc_type: update_string("cc_type", cc.cc_type),
    };

    println!("Updating credit card");

    println!("Making `update_credit_card` api call for guid {}", guid);
    Store::update_credit_card(store, guid.clone(), updatable)?;

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
        Command::AddAddress {} => run_add_address(&store),
        Command::GetAddress { guid } => run_get_address(&store, guid),
        Command::GetAllAddresses => run_get_all_addresses(&store),
        Command::UpdateAddress { guid } => run_update_address(&store, guid),
        Command::DeleteAddress { guid } => run_delete_address(&store, guid),

        Command::AddCreditCard {} => run_add_credit_card(&store),
        Command::GetCreditCard { guid } => run_get_credit_card(&store, guid),
        Command::GetAllCreditCards => run_get_all_credit_cards(&store),
        Command::UpdateCreditCard { guid } => run_update_credit_card(&store, guid),
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
