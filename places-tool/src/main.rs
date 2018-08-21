extern crate dirs;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate mentat;
extern crate rusqlite;

#[macro_use]
extern crate lazy_static;
extern crate rand;
extern crate env_logger;
extern crate clap;
extern crate tempfile;

use std::path::{Path, PathBuf};
use std::fs;

mod find_db;
mod anonymize;
mod to_mentat;


fn main() -> Result<(), failure::Error> {
    let matches = clap::App::new("places-tool")
        .subcommand(clap::SubCommand::with_name("to-mentat")
            .about("Convert a places database to a mentat database")
            .arg(clap::Arg::with_name("OUTPUT")
                .index(1)
                .help("Path where we should output the mentat db (defaults to ./mentat_places.db)"))
            .arg(clap::Arg::with_name("PLACES")
                .index(2)
                .help("Path to places.sqlite. If not provided, we'll use the largest places.sqlite in your firefox profiles"))
            .arg(clap::Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity (pass up to 3 times for more verbosity -- e.g. -vvv enables trace logs)"))
            .arg(clap::Arg::with_name("force")
                .short("f")
                .long("force")
                .help("Overwrite OUTPUT if it already exists"))
            .arg(clap::Arg::with_name("realistic")
                .short("r")
                .long("realistic")
                .help("Insert everything with one transaction per visit. This is a lot slower, \
                       but is a more realistic workload. It produces databases that are ~30% larger (for me).")))
        .subcommand(clap::SubCommand::with_name("anonymize")
            .about("Anonymize a places database")
            .arg(clap::Arg::with_name("OUTPUT")
                .index(1)
                .help("Path where we should output the anonymized db (defaults to places_anonymized.sqlite)"))
            .arg(clap::Arg::with_name("PLACES")
                .index(2)
                .help("Path to places.sqlite. If not provided, we'll use the largest places.sqlite in your firefox profiles"))
            .arg(clap::Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity (pass up to 3 times for more verbosity -- e.g. -vvv enables trace logs)"))
            .arg(clap::Arg::with_name("force")
                .short("f")
                .long("force")
                .help("Overwrite OUTPUT if it already exists")))
        .get_matches();

    let subcommand = matches.subcommand_name().map(|s| s.to_owned()).expect("Must provide subcommand");
    let is_anon = subcommand == "anonymize";
    let subcmd_matches = matches.subcommand_matches(&subcommand).unwrap();

    env_logger::init_from_env(match subcmd_matches.occurrences_of("v") {
        0 => env_logger::Env::default().filter_or("RUST_LOG", "warn"),
        1 => env_logger::Env::default().filter_or("RUST_LOG", "info"),
        2 => env_logger::Env::default().filter_or("RUST_LOG", "debug"),
        3 | _ => env_logger::Env::default().filter_or("RUST_LOG", "trace"),
    });

    let places_db = if let Some(places) = subcmd_matches.value_of("PLACES") {
        let meta = fs::metadata(&places)?;
        find_db::PlacesLocation {
            profile_name: "".into(),
            path: fs::canonicalize(places)?,
            db_size: meta.len(),
        }
    } else {
        let mut dbs = find_db::get_all_places_dbs()?;
        if dbs.len() == 0 {
            error!("No dbs found!");
            return Err(format_err!("No dbs found!"));
        }
        for p in &dbs {
            debug!("Found: profile {:?} with a {} places.sqlite", p.profile_name, p.friendly_db_size())
        }
        info!("Using profile {:?}", dbs[0].profile_name);
        dbs.into_iter().next().unwrap()
    };

    let out_db_path = subcmd_matches.value_of("OUTPUT").unwrap_or_else(|| {
        if is_anon {
            "./places_anonymized.sqlite"
        } else {
            "./mentat_places.db"
        }
    }).to_owned();

    if Path::new(&out_db_path).exists() {
        if subcmd_matches.is_present("force") {
            info!("Deleting previous `{}` because -f was passed", out_db_path);
            fs::remove_file(&out_db_path)?;
        } else {
            error!("{} already exists but `-f` argument was not provided", out_db_path);
            return Err(format_err!("Output path already exists"));
        }
    }

    if is_anon {
        let cmd = anonymize::AnonymizePlaces {
            input_path: places_db.path,
            output_path: PathBuf::from(out_db_path)
        };
        cmd.run()?;
    } else {
        let cmd = to_mentat::PlacesToMentat {
            mentat_db_path: PathBuf::from(out_db_path),
            places_db_path: places_db.path,
            realistic: subcmd_matches.is_present("realistic"),
        };
        cmd.run()?;
    }

    Ok(())
}
