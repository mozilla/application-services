/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use anyhow::Result;
use autofill::api::*;
use autofill::db::AutofillDb;
use std::{fs::File, io::BufReader};
use structopt::StructOpt;
use sync_guid::Guid;

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

    /// Gets Address from database
    #[structopt(name = "get-address")]
    GetAddress {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the address to retrieve
        guid: String,
    },

    /// Gets all Addresses from database
    #[structopt(name = "get-all-addresses")]
    GetAllAddresses,

    /// Update Address with given JSON address data
    #[structopt(name = "update-address")]
    UpdateAddress {
        #[structopt(name = "input-file", long, short = "i")]
        /// The input file containing the address data
        input_file: String,
    },

    /// Delete Address from database
    #[structopt(name = "delete-address")]
    DeleteAddress {
        #[structopt(name = "guid", long, short = "g")]
        /// The guid of the address to delete
        guid: String,
    },
}

fn run_add_address(db: &mut AutofillDb, filename: String) -> Result<()> {
    println!("Retrieving address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: NewAddressFields = serde_json::from_reader(reader)?;

    println!("Making `add_address` api call");
    let address = add_address(&mut db.writer, address_fields)?;

    println!("Created address: {:#?}", address);
    Ok(())
}

fn run_get_address(db: &mut AutofillDb, guid: String) -> Result<()> {
    println!("Getting address for guid `{}`", guid);

    let address = get_address(&mut db.writer, &Guid::from(guid))?;

    println!("Retrieved address: {:#?}", address);
    Ok(())
}

fn run_get_all_addresses(db: &mut AutofillDb) -> Result<()> {
    println!("Getting all addresses");

    let addresses = get_all_addresses(&mut db.writer)?;

    println!("Retrieved addresses: {:#?}", addresses);

    Ok(())
}

fn run_update_address(db: &mut AutofillDb, filename: String) -> Result<()> {
    println!("Updating address data from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let address_fields: Address = serde_json::from_reader(reader)?;
    let guid = address_fields.guid.clone();

    println!(
        "Making `update_address` api call for guid {}",
        guid.to_string()
    );
    update_address(&mut db.writer, address_fields)?;

    let address = get_address(&mut db.writer, &guid)?;
    println!("Updated address: {:#?}", address);

    Ok(())
}

fn run_delete_address(db: &mut AutofillDb, guid: String) -> Result<()> {
    println!("Deleting address for guid `{}`", guid);

    delete_address(&mut db.writer, &Guid::from(guid))?;

    println!("Successfully deleted address");
    Ok(())
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    if !opts.no_logging {
        cli_support::init_trace_logging();
    }

    let db_path = opts.database_path;
    let mut db = AutofillDb::new(db_path)?;

    match opts.cmd {
        Command::AddAddress { input_file } => run_add_address(&mut db, input_file),
        Command::GetAddress { guid } => run_get_address(&mut db, guid),
        Command::GetAllAddresses => run_get_all_addresses(&mut db),
        Command::UpdateAddress { input_file } => run_update_address(&mut db, input_file),
        Command::DeleteAddress { guid } => run_delete_address(&mut db, guid),
    }
}
