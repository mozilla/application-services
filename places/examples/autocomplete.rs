/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate places;

#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate failure;
extern crate rusqlite;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate url;
#[macro_use]
extern crate clap;
extern crate find_places_db;
extern crate tempfile;
extern crate termion;
extern crate rand;
use std::io::prelude::*;
// use rand::prelude::*;
use url::Url;
// use failure::Fail;
use places::{api::history, PageId, VisitObservation, VisitTransition};

use std::{fs, path::{Path, PathBuf}};

type Result<T> = std::result::Result<T, failure::Error>;


#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SerializedObservation {
    pub url: String, // This is actually required but we check after deserializing
    pub title: Option<String>,
    pub visit_type: Option<u32>,
    pub error: bool,
    pub is_redirect_source: bool,
    pub at: Option<u64>, // milliseconds
    pub referrer: Option<String>, // A URL
    pub remote: bool,
}

impl SerializedObservation {
    // We'd use TryFrom/TryInto but those are nightly only... :|
    pub fn into_visit(self) -> Result<VisitObservation> {
        let page_id = PageId::Url(Url::parse(&self.url)?);
        let mut obs = VisitObservation::new(page_id);
        if let Some(title) = self.title {
            obs = obs.title(title);
        }
        if let Some(visit_type) = self.visit_type.and_then(VisitTransition::from_primitive) {
            obs = obs.visit_type(visit_type);
        }
        if let Some(time) = self.at {
            obs = obs.at(places::Timestamp(time));
        }
        if self.error {
            obs = obs.is_error();
        }
        if self.remote {
            obs = obs.is_remote();
        }
        if self.is_redirect_source {
            obs = obs.is_permanent_redirect_source();
        }
        if let Some(referrer) = self.referrer {
            let referrer_url = Url::parse(&referrer)?;
            obs = obs.referrer(referrer_url);
        }
        Ok(obs)
    }
}

impl From<VisitObservation> for SerializedObservation {
    fn from(visit: VisitObservation) -> Self {
        Self {
            url: visit.get_url().expect("TODO: handle VisitObservation not based on URL").to_string(),
            title: visit.get_title().cloned(),
            visit_type: visit.get_visit_type().map(|vt| vt as u32),
            at: visit.get_at().map(|at| at.into()),
            error: visit.get_is_error(),
            is_redirect_source: visit.get_is_permanent_redirect_source(),
            remote: visit.get_is_remote(),
            referrer: visit.get_referrer().map(|url| url.to_string()),
        }
    }
}

#[derive(Default, Clone, Debug)]
struct ImportPlacesOptions {
    pub remote_probability: f64,
}

#[derive(Default, Debug, Clone)]
struct LegacyPlaceVisit {
    id: i64,
    date: i64,
    visit_type: u32,
    from_visit: i64,
}

#[derive(Default, Debug, Clone)]
struct LegacyPlace {
    id: i64,
    guid: String,
    url: String,
    title: Option<String>,
    hidden: i64,
    typed: i64,
    last_visit_date: i64,
    visit_count: i64,
    description: Option<String>,
    preview_image_url: Option<String>,
    visits: Vec<LegacyPlaceVisit>
}

impl LegacyPlace {
    pub fn from_row(row: &rusqlite::Row) -> Self {
        Self {
            id: row.get("place_id"),
            guid: row.get("place_guid"),
            title: row.get("place_title"),
            url: row.get("place_url"),
            description: row.get("place_description"),
            preview_image_url: row.get("place_preview_image_url"),
            typed: row.get("place_typed"),
            hidden: row.get("place_hidden"),
            visit_count: row.get("place_visit_count"),
            last_visit_date: row.get("place_last_visit_date"),
            visits: vec![
                LegacyPlaceVisit {
                    id: row.get("visit_id"),
                    date: row.get("visit_date"),
                    visit_type: row.get("visit_type"),
                    from_visit: row.get("visit_from_visit"),
                }
            ],
        }
    }
    pub fn insert(self, conn: &places::Connection, options: &ImportPlacesOptions) -> Result<()> {
        places::api::history::insert(conn, history::AddablePlaceInfo {
            page_id: PageId::Url(Url::parse(&self.url)?),
            title: self.title,
            // TODO: this should take a bunch of other things from `self`.
            visits: self.visits.into_iter().map(|v| history::AddableVisit {
                date: places::Timestamp((v.date / 1000) as u64),
                transition: VisitTransition::from_primitive(v.visit_type)
                                .unwrap_or(VisitTransition::Link),
                referrer: None,
                is_local: rand::random::<f64>() >= options.remote_probability,
            }).collect(),
        })?;
        Ok(())
    }
}

fn import_places(
    new: &places::Connection,
    old_path: PathBuf,
    options: ImportPlacesOptions
) -> Result<()> {
    let old = rusqlite::Connection::open_with_flags(&old_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let (place_count, visit_count) = {
        let mut stmt = old.prepare("SELECT count(*) FROM moz_places").unwrap();
        let mut rows = stmt.query(&[]).unwrap();
        let ps: i64 = rows.next().unwrap()?.get(0);

        let mut stmt = old.prepare("SELECT count(*) FROM moz_historyvisits").unwrap();
        let mut rows = stmt.query(&[]).unwrap();
        let vs: i64 = rows.next().unwrap()?.get(0);
        (ps, vs)
    };

    info!("Importing {} visits across {} places!", place_count, visit_count);
    let mut stmt = old.prepare("
        SELECT
            p.id                as place_id,
            p.guid              as place_guid,
            p.url               as place_url,
            p.title             as place_title,

            p.hidden            as place_hidden,
            p.typed             as place_typed,
            p.last_visit_date   as place_last_visit_date,
            p.visit_count       as place_visit_count,

            p.description       as place_description,
            p.preview_image_url as place_preview_image_url,

            v.id                as visit_id,
            v.visit_date        as visit_date,
            v.visit_type        as visit_type,
            v.from_visit        as visit_from_visit
        FROM moz_places p
        JOIN moz_historyvisits v
            ON p.id = v.place_id
        ORDER BY p.id
    ")?;

    let mut rows = stmt.query(&[])?;
    let mut current_place = LegacyPlace { id: -1, .. LegacyPlace::default() };
    let mut place_counter = 0;

    print!("Processing {} / {} places (approx.)", place_counter, place_count);
    let _ = std::io::stdout().flush();
    while let Some(row_or_error) = rows.next() {
        let row = row_or_error?;
        let id: i64 = row.get("place_id");
        if current_place.id == id {
            current_place.visits.push(LegacyPlaceVisit {
                id: row.get("visit_id"),
                date: row.get("visit_date"),
                visit_type: row.get("visit_type"),
                from_visit: row.get("visit_from_visit"),
            });
            continue;
        }
        place_counter += 1;
        print!("\rProcessing {} / {} places (approx.)", place_counter, place_count);
        let _ = std::io::stdout().flush();
        if current_place.id != -1 {
            current_place.insert(new, &options)?;
        }
        current_place = LegacyPlace::from_row(&row);
    }
    if current_place.id != -1 {
        current_place.insert(new, &options)?;
    }
    println!("Finished processing records");
    info!("Finished import!");
    Ok(())
}

fn read_json_file<T>(path: impl AsRef<Path>) -> Result<T> where for<'a> T: serde::de::Deserialize<'a> {
    let file = fs::File::open(path.as_ref())?;
    Ok(serde_json::from_reader(&file)?)
}

fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(
        env_logger::Env::default().filter_or("RUST_LOG", spec)
    );
}

fn main() -> Result<()> {
    init_logging();
    let matches = clap::App::new("autocomplete-example")
        .arg(clap::Arg::with_name("database_path")
            .long("database")
            .short("d")
            .help("Path to the database (with the *new* schema). Defaults to './new-places.db'"))
        .arg(clap::Arg::with_name("encryption_key")
            .long("encryption-key")
            .short("k")
            .help("Encryption key to use with the database. Leave blank for unencrypted"))
        .arg(clap::Arg::with_name("import_places")
            .long("import-places")
            .short("p")
            .takes_value(true)
            .value_name("'auto'|'path/to/places.sqlite'")
            .help("Source places db to import from, or 'auto' to import from the largest places.sqlite"))
        .arg(clap::Arg::with_name("import_places_remote_weight")
            .long("import-places-remote-weight")
            .takes_value(true)
            .value_name("WEIGHT")
            .help("Probability (between 0.0 and 1.0, default = 0.1) that a given visit from `places` should \
                   be considered `remote`. Ignored when --import-places is not passed"))
        .arg(clap::Arg::with_name("import_observations")
            .long("import-observations")
            .short("o")
            .takes_value(true)
            .help("Path to a JSON file containing a list of 'observations'"))
        .get_matches();

    let db_path = matches.value_of("database_path").unwrap_or("./new-places.db");
    let encryption_key = matches.value_of("encryption_key");

    let conn = places::Connection::new(db_path, encryption_key)?;

    if let Some(import_places_arg) = matches.value_of("import_places") {
        let options = ImportPlacesOptions {
            remote_probability: value_t!(matches,
                "import_places_remote_weight", f64).unwrap_or(0.1),
        };
        let import_source = if import_places_arg == "auto" {
            info!("Automatically locating largest places DB in your profile(s)");
            let profile_info = if let Some(info) = find_places_db::get_largest_places_db()? {
                info
            } else {
                error!("Failed to locate your firefox profile!");
                bail!("--import-places=auto specified, but couldn't find a `places.sqlite`");
            };
            info!("Using a {} places.sqlite from profile '{}' (places path = {:?})",
                  profile_info.friendly_db_size(),
                  profile_info.profile_name,
                  profile_info.path);
            assert!(profile_info.path.exists(),
                    "Bug in find_places_db, provided path doesn't exist!");
            profile_info.path
        } else {
            let path = Path::new(import_places_arg);
            if !path.exists() {
                bail!("Provided path to --import-places doesn't exist and isn't 'auto': {:?}",
                      import_places_arg);
            }
            path.to_owned()
        };

        // Copy `import_source` to a temporary location, because we aren't allowed to open
        // places.sqlite while Firefox is open.

        let dir = tempfile::tempdir()?;
        let temp_places = dir.path().join("places-tmp.sqlite");

        fs::copy(&import_source, &temp_places)?;
        import_places(&conn, temp_places, options)?;
    }

    if let Some(observations_json) = matches.value_of("import_observations") {
        info!("Importing observations from {}", observations_json);
        let observations: Vec<SerializedObservation> = read_json_file(observations_json)?;
        let num_observations = observations.len();
        info!("Found {} observations", num_observations);
        let mut counter = 0;
        for obs in observations {
            let visit = obs.into_visit()?;
            places::apply_observation(&conn, visit)?;
            counter += 1;
            if (counter % 1000) == 0 {
                trace!("Importing observations {} / {}", counter, num_observations);
            }
        }
    }

    Ok(())
}
