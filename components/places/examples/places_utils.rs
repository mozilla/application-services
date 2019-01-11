/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use places::storage::bookmarks::{
    fetch_tree, insert_tree, BookmarkNode, BookmarkRootGuid, BookmarkTreeNode, FolderNode,
    SeparatorNode,
};
use places::types::{BookmarkType, SyncGuid, Timestamp};
use places::PlacesDb;

use serde_derive::*;
use sql_support::ConnExt;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use url::Url;

type Result<T> = std::result::Result<T, failure::Error>;

fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

// A struct in the format of desktop with a union of all fields.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
#[allow(non_snake_case)]
struct DesktopMapping {
    typeCode: u8,
    guid: Option<SyncGuid>,
    dateAdded: Option<u64>,
    lastModified: Option<u64>,
    title: Option<String>,
    #[serde(with = "url_serde")]
    uri: Option<Url>,
    children: Vec<DesktopMapping>,
}

fn convert_timestamp(t: Option<u64>) -> Option<Timestamp> {
    match t {
        None => None,
        Some(v) => Some(Timestamp(v / 1000)),
    }
}

fn convert_node(dm: DesktopMapping) -> Option<BookmarkTreeNode> {
    // this patten has been copy-pasta'd too often...
    let bookmark_type = match BookmarkType::from_u8(dm.typeCode) {
        Some(t) => t,
        None => match dm.uri {
            Some(_) => BookmarkType::Bookmark,
            _ => BookmarkType::Folder,
        },
    };
    Some(match bookmark_type {
        BookmarkType::Bookmark => {
            let url = match dm.uri {
                Some(uri) => uri,
                None => {
                    log::warn!("ignoring bookmark node without url: {:?}", dm);
                    return None;
                }
            };
            BookmarkTreeNode::Bookmark(BookmarkNode {
                guid: dm.guid,
                date_added: convert_timestamp(dm.dateAdded),
                last_modified: convert_timestamp(dm.lastModified),
                title: dm.title,
                url,
            })
        }
        BookmarkType::Separator => BookmarkTreeNode::Separator(SeparatorNode {
            guid: dm.guid,
            date_added: convert_timestamp(dm.dateAdded),
            last_modified: convert_timestamp(dm.lastModified),
        }),
        BookmarkType::Folder => BookmarkTreeNode::Folder(FolderNode {
            guid: dm.guid,
            date_added: convert_timestamp(dm.dateAdded),
            last_modified: convert_timestamp(dm.lastModified),
            title: dm.title,
            children: dm
                .children
                .into_iter()
                .filter_map(|c| convert_node(c))
                .collect(),
        }),
    })
}

fn do_import(db: &PlacesDb, root: BookmarkTreeNode) -> Result<()> {
    // We need to import each of the sub-trees individually.
    // Later we will want to get smarter around guids - currently we will
    // fail to do this twice due to guid dupes - but that's OK for now.
    let folder = match root {
        BookmarkTreeNode::Folder(folder_node) => folder_node,
        _ => {
            println!("Imported node isn't a folder structure");
            return Ok(());
        }
    };
    let is_root = match folder.guid {
        Some(ref guid) => BookmarkRootGuid::Root == *guid,
        None => false,
    };
    if !is_root {
        // later we could try and import a sub-tree.
        println!("Imported tree isn't the root node");
        return Ok(());
    }

    for sub_root_node in folder.children {
        let sub_root_folder = match sub_root_node {
            BookmarkTreeNode::Folder(folder_node) => folder_node,
            _ => {
                println!("Child of the root isn't a folder - skipping...");
                continue;
            }
        };
        println!("importing {:?}", sub_root_folder.guid);
        insert_tree(db, &sub_root_folder)?
    }
    Ok(())
}

fn run_desktop_import(db: &PlacesDb, matches: &clap::ArgMatches) -> Result<()> {
    let filename = matches.value_of("input-file").unwrap();
    println!("import from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let m: DesktopMapping = serde_json::from_reader(reader)?;
    // convert mapping into our tree.
    let root = match convert_node(m) {
        Some(node) => node,
        None => {
            println!("Failed to read a tree from this file");
            return Ok(());
        }
    };
    do_import(db, root)
}

fn run_native_import(db: &PlacesDb, matches: &clap::ArgMatches) -> Result<()> {
    let filename = matches.value_of("input-file").unwrap();
    println!("import from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let root: BookmarkTreeNode = serde_json::from_reader(reader)?;
    do_import(db, root)
}

fn run_native_export(db: &PlacesDb, matches: &clap::ArgMatches) -> Result<()> {
    let filename = matches.value_of("output-file").unwrap();
    println!("export to {}", filename);

    let file = File::create(filename)?;
    let writer = BufWriter::new(file);

    let tree = fetch_tree(db.conn(), &BookmarkRootGuid::Root.into())?.unwrap();
    serde_json::to_writer_pretty(writer, &tree)?;
    Ok(())
}

fn main() -> Result<()> {
    init_logging();

    let matches = clap::App::new("places_utils")
        .about("Command-line utilities for places")
        .arg(
            clap::Arg::with_name("database_path")
                .short("d")
                .long("database")
                .value_name("DATABASE")
                .takes_value(true)
                .help("Path to the database (default: \"./new-places.db\")"),
        )
        .arg(
            clap::Arg::with_name("encryption_key")
                .short("k")
                .long("key")
                .value_name("ENCRYPTION_KEY")
                .takes_value(true)
                .help("Database encryption key."),
        )
        .subcommand(
            clap::SubCommand::with_name("export-bookmarks")
                .about("Exports bookmarks (but not in a way Desktop can import it!)")
                .arg(
                    clap::Arg::with_name("output-file")
                        .short("o")
                        .value_name("FILE")
                        .takes_value(true)
                        .help("The name of the json file to export to from")
                        .required(true),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name("import-bookmarks")
                .about("Import bookmarks from a 'native' export (ie, as exported by this utility)")
                .arg(
                    clap::Arg::with_name("input-file")
                        .short("i")
                        .value_name("FILE")
                        .takes_value(true)
                        .help("The name of the json file to import from")
                        .required(true),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name("import-desktop-bookmarks")
                .about("Imports bookmarks from a desktop export")
                .arg(
                    clap::Arg::with_name("input-file")
                        .short("i")
                        .value_name("FILE")
                        .takes_value(true)
                        .help("The name of the json file to import from")
                        .required(true),
                ),
        )
        .get_matches();

    let db_path = matches
        .value_of("database_path")
        .unwrap_or("./new-places.db");
    let db = PlacesDb::open(db_path, matches.value_of("encryption_key"))?;

    match matches.subcommand() {
        ("export-bookmarks", Some(m)) => run_native_export(&db, m),
        ("import-bookmarks", Some(m)) => run_native_import(&db, m),
        ("import-desktop-bookmarks", Some(m)) => run_desktop_import(&db, m),
        _ => Ok(()),
    }
}
