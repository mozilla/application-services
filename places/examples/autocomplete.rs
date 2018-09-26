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

use std::thread;
use std::time::{Instant, Duration};
use std::sync::{mpsc, atomic::{AtomicUsize, Ordering}, Arc};

use std::io::prelude::*;
// use rand::prelude::*;
use url::Url;
// use failure::Fail;
use places::{
    api::{
        history,
        autocomplete::{
            self,
            SearchParams,
            SearchResult,
        }
    },
    PageId,
    VisitObservation,
    VisitTransition,
};

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

#[derive(Debug, Clone)]
struct ConnectionArgs {
    path: PathBuf,
    encryption_key: Option<String>
}

impl ConnectionArgs {
    pub fn connect(&self) -> Result<places::Connection> {
        let key = match &self.encryption_key {
            Some(k) => Some(k.as_str()),
            _ => None,
        };
        // TODO: it would be nice if this could be a read-only connection.
        Ok(places::Connection::new(&self.path, key)?)
    }
}

#[derive(Debug, Clone)]
struct AutocompleteRequest {
    id: usize,
    search: SearchParams,
}

#[derive(Debug, Clone)]
struct AutocompleteResponse {
    id: usize,
    search: SearchParams,
    results: Vec<SearchResult>,
    took: Duration,
}

struct BackgroundAutocomplete {
    // Only written from the main thread, and read from the background thread.
    // We use this to signal to the background thread that it shouldn't start on a query that has
    // an ID below this value, since we already have added a newer one into the queue. Note that
    // an ID higher than this value is allowed (it indicates that the BG thread is reading in the
    // window between when we added the search to the queue and when we )
    last_id: Arc<AtomicUsize>,
    // Write-only interface to the queue that the BG thread reads from.
    send_query: mpsc::Sender<AutocompleteRequest>,
    // Read-only interface to the queue the BG thread returns results from.
    recv_results: mpsc::Receiver<AutocompleteResponse>,
    // Currently not used but if we wanted to restart the thread or start additional threads
    // we could use this.
    // conn_args: ConnectionArgs,
    // Thread handle for the BG thread. We can't drop this without problems so we
    // prefix with _ to shut rust up about it being unused.
    _handle: thread::JoinHandle<Result<()>>,
}

impl BackgroundAutocomplete {
    pub fn start(conn_args: ConnectionArgs) -> Result<Self> {
        let (send_query, recv_query) = mpsc::channel::<AutocompleteRequest>();

        // Should this channel have a buffer?
        let (send_results, recv_results) = mpsc::channel::<AutocompleteResponse>();

        let last_id = Arc::new(AtomicUsize::new(0usize));

        let handle = {
            let last_id = last_id.clone();
            let conn_args = conn_args.clone();
            thread::spawn(move || {
                // Note: unwraps/panics here won't bring down the main thread.
                let conn = conn_args.connect().expect("Failed to open connection on BG thread");
                for AutocompleteRequest { id, search } in recv_query.iter() {
                    // Check if this query is worth processing. Note that we check that the id
                    // isn't known to be stale. The id can be ahead of `last_id`, since
                    // we push the item on before incrementing `last_id`.
                    if id < last_id.load(Ordering::SeqCst) {
                        continue;
                    }
                    let start = Instant::now();
                    match autocomplete::search_frecent(&conn, search.clone()) {
                        Ok(results) => {
                            // Should we skip sending results if `last_id` indicates we
                            // don't care anymore?
                            send_results.send(AutocompleteResponse {
                                id,
                                search,
                                results,
                                took: Instant::now().duration_since(start)
                            }).unwrap(); // This failing means the main thread has died (most likely)
                        }
                        Err(e) => {
                            // TODO: this is likely not to go very well since we're in raw mode...
                            error!("Got error doing autocomplete: {:?}", e);
                            panic!("Got error doing autocomplete: {:?}", e);
                            // return Err(e.into());
                        }
                    }
                }
                Ok(())
            })
        };

        Ok(BackgroundAutocomplete {
            last_id,
            send_query,
            recv_results,
            // conn_args,
            _handle: handle,
        })
    }

    pub fn query(&mut self, search: SearchParams) -> Result<()> {
        // Cludgey but whatever.
        let id = self.last_id.load(Ordering::SeqCst) + 1;
        let request = AutocompleteRequest { id, search };
        let res = self.send_query.send(request);
        self.last_id.store(id, Ordering::SeqCst);
        res?;
        Ok(())
    }

    pub fn poll_results(&mut self) -> Result<Option<AutocompleteResponse>> {
        match self.recv_results.try_recv() {
            Ok(results) => Ok(Some(results)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

fn start_autocomplete(db_path: PathBuf, encryption_key: Option<&str>) -> Result<()> {
    use termion::{
        event::Key,
        input::TermRead,
        raw::IntoRawMode,
        clear,
        cursor::{self, Goto},
    };

    let mut autocompleter = BackgroundAutocomplete::start(ConnectionArgs {
        path: db_path,
        encryption_key: encryption_key.map(|s| s.to_owned())
    })?;

    let mut stdin = termion::async_stdin();
    let stdout = std::io::stdout().into_raw_mode()?;
    let mut stdout = termion::screen::AlternateScreen::from(stdout);
    write!(stdout, "{}{}Autocomplete demo (press escape to exit){}> ",
           clear::All, Goto(1, 1), Goto(1, 2))?;
    stdout.flush()?;

    let no_title = format!("{}(no title){}", termion::style::Faint, termion::style::NoFaint);
    // TODO: refactor these to be part of a struct or something.
    let mut query_str = String::new();
    let mut last_query = Instant::now();
    let mut results: Option<AutocompleteResponse> = None;
    let mut pos = 0;
    let mut cursor_idx = 0;
    let mut repaint_results = true;
    let mut input_changed = true;
    loop {
        for res in (&mut stdin).keys() {//.events_and_raw() {
            let key = res?;
            match key {
                Key::Esc => {
                    return Ok(())
                }
                Key::Char('\n') | Key::Char('\r') => {
                    if !query_str.is_empty() {
                        last_query = Instant::now();
                        autocompleter.query(SearchParams {
                            search_string: format!("%{}%", query_str), // XXX
                            limit: 5,
                        })?;
                    }
                }
                Key::Char(ch) => {
                    query_str.insert(cursor_idx, ch);
                    cursor_idx += 1;
                    input_changed = true;
                }
                Key::Ctrl('n') | Key::Down => {
                    if let Some(res) = &results {
                        if pos + 1 < res.results.len() {
                            pos = pos + 1;
                            repaint_results = true;
                        }
                    }
                }
                Key::Ctrl('p') | Key::Up => {
                    if results.is_some() && pos > 0 {
                        pos = pos - 1;
                        repaint_results = true;
                    }
                }
                Key::Ctrl('k') => {
                    query_str.truncate(cursor_idx);
                    input_changed = true;
                }
                Key::Right | Key::Ctrl('f') => {
                    if cursor_idx < query_str.len() {
                        write!(stdout, "{}", termion::cursor::Right(1))?;
                        cursor_idx += 1;
                    }
                }
                Key::Left | Key::Ctrl('b') => {
                    if cursor_idx > 0 {
                        write!(stdout, "{}", termion::cursor::Left(1))?;
                        cursor_idx -= 1;
                    }
                }
                Key::Backspace => {
                    if cursor_idx > 0 {
                        query_str.remove(cursor_idx - 1);
                        cursor_idx -= 1;
                        input_changed = true;
                    }
                }
                Key::Delete | Key::Ctrl('d') => {
                    if cursor_idx + 1 != query_str.len() {
                        query_str.remove(cursor_idx + 1);
                        input_changed = true;
                    }
                }
                Key::Ctrl('a') | Key::Home => {
                    write!(stdout, "{}", Goto(3, 2));
                    cursor_idx = 0;
                }
                Key::Ctrl('e') | Key::End => {
                    write!(stdout, "{}", Goto(3 + query_str.len() as u16, 2));
                    cursor_idx = query_str.len();
                }
                Key::Ctrl('u') => {
                    cursor_idx = 0;
                    query_str.clear();
                    input_changed = true;
                }
                _ => {}
            }
        }
        if let Some(new_res) = autocompleter.poll_results()? {
            results = Some(new_res);
            pos = 0;
            repaint_results = true;
        }
        if input_changed {
            let now = Instant::now();
            let last = last_query;
            last_query = now;
            if !query_str.is_empty() && now.duration_since(last) > Duration::from_millis(100) {
                autocompleter.query(SearchParams {
                    search_string: format!("%{}%", query_str), // XXX
                    limit: 5,
                })?;
            }
            write!(stdout, "{}{}> {}{}",
                Goto(1, 2),
                clear::CurrentLine,
                query_str,
                Goto(3 + cursor_idx as u16, 2))?;

            if query_str.is_empty() {
                results = None;
                pos = 0;
                repaint_results = true;
            }
            input_changed = false;
        }


        if repaint_results {
            match &results {
                Some(results) => {
                    write!(stdout, "{}{}{}Query id={} gave {} results (max {}) for \"{}\" after {}us",
                        cursor::Save, Goto(1, 3), clear::AfterCursor,
                        results.id,
                        results.results.len(),
                        results.search.limit,
                        results.search.search_string,
                        results.took.as_secs() * 1_000_000 + (results.took.subsec_nanos() as u64 / 1000)
                    )?;
                    write!(stdout, "{}", Goto(1, 4));
                    for (i, item) in results.results.iter().enumerate() {
                        write!(stdout, "{}", Goto(1, 4 + (i as u16) * 2))?;
                        if i == pos {
                            write!(stdout, "{}", termion::style::Invert)?;
                        }
                        write!(stdout, "{}. {}", i + 1, item.title.as_ref().unwrap_or(&no_title))?;
                        write!(stdout, "{}    {}", Goto(1, 5 + (i as u16) * 2), item.url.to_string())?;
                        if i == pos {
                            write!(stdout, "{}", termion::style::NoInvert)?;
                        }
                    }
                    write!(stdout, "{}", cursor::Restore)?;
                }
                None => {
                    write!(stdout, "{}{}{}{}", cursor::Save, Goto(1, 3), clear::AfterCursor, cursor::Restore)?;
                }
            }
            repaint_results = false;
        }
        stdout.flush()?;
        thread::sleep(Duration::from_millis(16));
    }
}

fn main() -> Result<()> {

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

    let conn = places::Connection::new(&db_path, encryption_key)?;

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
    // Close our connection before starting autocomplete.
    drop(conn);
    start_autocomplete(Path::new(db_path).to_owned(), encryption_key)?;

    Ok(())
}
