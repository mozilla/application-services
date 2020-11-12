/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use anyhow::Result;
use autofill::db::{
    models::{address, credit_card},
    store::Store,
};
use std::{fs::File, io::BufReader};
use structopt::StructOpt;

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
}

fn run_add_address(store: &Store, filename: String) -> Result<()> {
    println!("Retrieving address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: address::NewAddressFields = serde_json::from_reader(reader)?;

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

fn run_update_address(store: &Store, filename: String) -> Result<()> {
    println!("Updating address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: address::Address = serde_json::from_reader(reader)?;
    let guid = address_fields.guid.clone();

    println!("Making `update_address` api call for guid {}", guid);
    Store::update_address(store, address_fields)?;

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
    let credit_card_fields: credit_card::NewCreditCardFields = serde_json::from_reader(reader)?;

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

fn run_update_credit_card(store: &Store, filename: String) -> Result<()> {
    println!("Updating credit card data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let credit_card_fields: credit_card::CreditCard = serde_json::from_reader(reader)?;
    let guid = credit_card_fields.guid.clone();

    println!("Making `update_credit_card` api call for guid {}", guid);
    Store::update_credit_card(store, credit_card_fields)?;

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

fn main() -> Result<()> {
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
        Command::UpdateAddress { input_file } => run_update_address(&store, input_file),
        Command::DeleteAddress { guid } => run_delete_address(&store, guid),

        Command::AddCreditCard { input_file } => run_add_credit_card(&store, input_file),
        Command::GetCreditCard { guid } => run_get_credit_card(&store, guid),
        Command::GetAllCreditCards => run_get_all_credit_cards(&store),
        Command::UpdateCreditCard { input_file } => run_update_credit_card(&store, input_file),
        Command::DeleteCreditCard { guid } => run_delete_credit_card(&store, guid),
    }
}
