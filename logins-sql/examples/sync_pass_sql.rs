/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![recursion_limit = "4096"]

extern crate logins_sql;
extern crate sync15_adapter as sync;
extern crate fxa_client;
extern crate url;
#[macro_use]
extern crate prettytable;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate rusqlite;

extern crate webbrowser;
extern crate clap;

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate chrono;
extern crate failure;

use failure::Fail;

use std::{fs, io::{self, Read, Write}};
use std::collections::HashMap;
use fxa_client::{FirefoxAccount, Config, AccessTokenInfo};
use sync::{Sync15StorageClientInit, KeyBundle};
use logins_sql::{PasswordEngine, Login};

const CLIENT_ID: &str = "98adfa37698f255b";
const REDIRECT_URI: &str = "https://lockbox.firefox.com/fxa/ios-redirect.html";

const CONTENT_BASE: &str = "https://accounts.firefox.com";
const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

// I'm completely punting on good error handling here.
type Result<T> = std::result::Result<T, failure::Error>;

#[derive(Debug, Deserialize)]
struct ScopedKeyData {
    k: String,
    kty: String,
    kid: String,
    scope: String,
}

fn load_fxa_creds(path: &str) -> Result<FirefoxAccount> {
    let mut file = fs::File::open(path)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn load_or_create_fxa_creds(path: &str, cfg: Config) -> Result<FirefoxAccount> {
    load_fxa_creds(path)
    .or_else(|e| {
        info!("Failed to load existing FxA credentials from {:?} (error: {}), launching OAuth flow", path, e);
        create_fxa_creds(path, cfg)
    })
}

fn create_fxa_creds(path: &str, cfg: Config) -> Result<FirefoxAccount> {
    let mut acct = FirefoxAccount::new(cfg);
    let oauth_uri = acct.begin_oauth_flow(&[SYNC_SCOPE], true)?;

    if let Err(_) = webbrowser::open(&oauth_uri.as_ref()) {
        warn!("Failed to open a web browser D:");
        println!("Please visit this URL, sign in, and then copy-paste the final URL below.");
        println!("\n    {}\n", oauth_uri);
    } else {
        println!("Please paste the final URL below:\n");
    }

    let final_url = url::Url::parse(&prompt_string("Final URL").unwrap_or(String::new()))?;
    let query_params = final_url.query_pairs().into_owned().collect::<HashMap<String, String>>();

    acct.complete_oauth_flow(&query_params["code"], &query_params["state"])?;
    let mut file = fs::File::create(path)?;
    write!(file, "{}", acct.to_json()?)?;
    file.flush()?;
    Ok(acct)
}

fn prompt_string<S: AsRef<str>>(prompt: S) -> Option<String> {
    print!("{}: ", prompt.as_ref());
    let _ = io::stdout().flush(); // Don't care if flush fails really.
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("Failed to read line...");
    if let Some('\n') = s.chars().next_back() { s.pop(); }
    if let Some('\r') = s.chars().next_back() { s.pop(); }
    if s.len() == 0 {
        None
    } else {
        Some(s)
    }
}

fn prompt_usize<S: AsRef<str>>(prompt: S) -> Option<usize> {
    if let Some(s) = prompt_string(prompt) {
        match s.parse::<usize>() {
            Ok(n) => Some(n),
            Err(_) => {
                println!("Couldn't parse!");
                None
            }
        }
    } else {
        None
    }
}

fn read_login() -> Login {
    let username = prompt_string("username").unwrap_or_default();
    let password = prompt_string("password").unwrap_or_default();
    let form_submit_url = prompt_string("form_submit_url");
    let hostname = prompt_string("hostname").unwrap_or_default();
    let http_realm = prompt_string("http_realm");
    let username_field = prompt_string("username_field").unwrap_or_default();
    let password_field = prompt_string("password_field").unwrap_or_default();
    let record = Login {
        id: sync::util::random_guid().unwrap().into(),
        username,
        password,
        username_field,
        password_field,
        form_submit_url,
        http_realm,
        hostname,
        .. Login::default()
    };

    if let Err(e) = record.check_valid() {
        warn!("Warning: produced invalid record: {}", e);
    }
    record
}

fn update_string(field_name: &str, field: &mut String, extra: &str) -> bool {
    let opt_s = prompt_string(format!("new {} [now {}{}]", field_name, field, extra));
    if let Some(s) = opt_s {
        *field = s;
        true
    } else {
        false
    }
}

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(|s| s.as_ref())
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn update_login(record: &mut Login) {
    update_string("username", &mut record.username, ", leave blank to keep");
    update_string("password", &mut record.password, ", leave blank to keep");
    update_string("hostname", &mut record.hostname, ", leave blank to keep");

    update_string("username_field", &mut record.username_field, ", leave blank to keep");
    update_string("password_field", &mut record.password_field, ", leave blank to keep");

    if prompt_bool(&format!("edit form_submit_url? (now {}) [yN]", string_opt_or(&record.form_submit_url, "(none)"))).unwrap_or(false) {
        record.form_submit_url = prompt_string("form_submit_url");
    }

    if prompt_bool(&format!("edit http_realm? (now {}) [yN]", string_opt_or(&record.http_realm, "(none)"))).unwrap_or(false) {
        record.http_realm = prompt_string("http_realm");
    }

    if let Err(e) = record.check_valid() {
        warn!("Warning: produced invalid record: {}", e);
    }
}

fn prompt_bool(msg: &str) -> Option<bool> {
    let result = prompt_string(msg);
    result.and_then(|r| match r.chars().next().unwrap() {
        'y' | 'Y' | 't' | 'T' => Some(true),
        'n' | 'N' | 'f' | 'F' => Some(false),
        _ => None
    })
}

fn prompt_chars(msg: &str) -> Option<char> {
    prompt_string(msg).and_then(|r| r.chars().next())
}

fn timestamp_to_string(milliseconds: i64) -> String {
    use chrono::{Local, DateTime};
    use std::time::{UNIX_EPOCH, Duration};
    let time = UNIX_EPOCH + Duration::from_millis(milliseconds as u64);
    let dtl: DateTime<Local> = time.into();
    dtl.format("%l:%M:%S %p%n%h %e, %Y").to_string()
}

fn show_sql(e: &PasswordEngine, sql: &str) -> Result<()> {
    use prettytable::{row::Row, cell::Cell, Table};
    use rusqlite::types::Value;
    let conn = e.conn();
    let mut stmt = conn.prepare(sql)?;
    let cols: Vec<String> = stmt.column_names().into_iter().map(|x| x.to_owned()).collect();
    let len = cols.len();
    let mut table = Table::new();
    table.add_row(Row::new(
        cols.iter().map(|name| Cell::new(&name).style_spec("bc")).collect()
    ));

    let rows = stmt.query_map(&[], |row| {
        (0..len).into_iter().map(|idx| {
            match row.get::<_, Value>(idx) {
                Value::Null => Cell::new("null").style_spec("Fd"),
                Value::Integer(i) => Cell::new(&i.to_string()).style_spec("Fb"),
                Value::Real(r) => Cell::new(&r.to_string()).style_spec("Fb"),
                Value::Text(s) => Cell::new(&s.to_string()).style_spec("Fr"),
                Value::Blob(b) => Cell::new(&format!("{}b blob", b.len()))
            }
        }).collect::<Vec<_>>()
    })?;

    for row in rows {
        table.add_row(Row::new(row?));
    }
    table.printstd();
    Ok(())
}

fn show_all(engine: &PasswordEngine) -> Result<Vec<String>> {
    let records = engine.list()?;

    let mut table = prettytable::Table::new();

    table.add_row(row![bc =>
        "(idx)",
        "Guid",
        "Username",
        "Password",
        "Host",

        "Submit URL",
        "HTTP Realm",

        "User Field",
        "Pass Field",

        "Uses",
        "Created At",
        "Changed At",
        "Last Used"
    ]);

    let mut v = Vec::with_capacity(records.len());
    let mut record_copy = records.clone();
    record_copy.sort_by(|a, b| a.id.cmp(&b.id));
    for rec in records.iter() {
        table.add_row(row![
            r->v.len(),
            Fr->&rec.id,
            &rec.username,
            Fd->&rec.password,

            &rec.hostname,
            string_opt_or(&rec.form_submit_url, ""),
            string_opt_or(&rec.http_realm, ""),

            &rec.username_field,
            &rec.password_field,

            rec.times_used,
            timestamp_to_string(rec.time_created),
            timestamp_to_string(rec.time_password_changed),
            if rec.time_last_used == 0 {
                "Never".to_owned()
            } else {
                timestamp_to_string(rec.time_last_used)
            }
        ]);
        v.push(rec.id.clone());
    }
    table.printstd();
    Ok(v)
}

fn prompt_record_id(e: &PasswordEngine, action: &str) -> Result<Option<String>> {
    let index_to_id = show_all(e)?;
    let input = if let Some(input) = prompt_usize(&format!("Enter (idx) of record to {}", action)) {
        input
    } else {
        return Ok(None);
    };
    if input >= index_to_id.len() {
        info!("No such index");
        return Ok(None);
    }
    Ok(Some(index_to_id[input].clone()))
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
    std::env::set_var("RUST_BACKTRACE", "1");

    let matches = clap::App::new("sync_pass_sql")
        .about("CLI login syncing tool (backed by sqlcipher)")

        .arg(clap::Arg::with_name("database_path")
            .short("d")
            .long("database")
            .value_name("LOGINS_DATABASE")
            .takes_value(true)
            .help("Path to the logins database (default: \"./logins.db\")"))

        .arg(clap::Arg::with_name("encryption_key")
            .short("k")
            .long("key")
            .value_name("ENCRYPTION_KEY")
            .takes_value(true)
            .help("Database encryption key.")
            .required(true))

        .arg(clap::Arg::with_name("credential_file")
            .short("c")
            .long("credentials")
            .value_name("CREDENTIAL_JSON")
            .takes_value(true)
            .help("Path to store our cached fxa credentials (defaults to \"./credentials.json\""))

        .get_matches();

    let cred_file = matches.value_of("credential_file").unwrap_or("./credentials.json");
    let db_path = matches.value_of("database_path").unwrap_or("./logins.db");
    // This should already be checked by `clap`, IIUC
    let encryption_key = matches.value_of("encryption_key").expect("Encryption key is not optional");

    // Lets not log the encryption key, it's just not a good habit to be in.
    debug!("Using credential file = {:?}, db = {:?}", cred_file, db_path);

    // TODO: allow users to use stage/etc.
    let cfg = Config::import_from(CONTENT_BASE, CLIENT_ID, REDIRECT_URI)?;
    let tokenserver_url = cfg.token_server_endpoint_url()?;

    // TODO: we should probably set a persist callback on acct?
    let mut acct = load_or_create_fxa_creds(cred_file, cfg.clone())?;
    let token_info: AccessTokenInfo = match acct.get_access_token(SYNC_SCOPE) {
        Ok(t) => t,
        Err(_) => {
            // The cached credentials did not have appropriate scope, sign in again.
            warn!("Credentials do not have appropriate scope, launching OAuth flow.");
            acct = create_fxa_creds(cred_file, cfg.clone())?;
            acct.get_access_token(SYNC_SCOPE)?
        }
    };

    let key: ScopedKeyData = serde_json::from_str(&token_info.key.unwrap())?;

    let client_init = Sync15StorageClientInit {
        key_id: key.kid.clone(),
        access_token: token_info.token.clone(),
        tokenserver_url,
    };
    let root_sync_key = KeyBundle::from_ksync_base64(&key.k)?;

    let engine = PasswordEngine::new(db_path, Some(encryption_key))?;

    info!("Engine has {} passwords", engine.list()?.len());

    if let Err(e) = show_all(&engine) {
        warn!("Failed to show initial login data! {}", e);
    }

    loop {
        match prompt_chars("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew, [R]eset, [W]ipe, [T]ouch, E[x]ecute SQL Query, or [Q]uit").unwrap_or('?') {
            'A' | 'a' => {
                info!("Adding new record");
                let record = read_login();
                if let Err(e) = engine.add(record) {
                    warn!("Failed to create record! {}", e);
                }
            }
            'D' | 'd' => {
                info!("Deleting record");
                match prompt_record_id(&engine, "delete") {
                    Ok(Some(id)) => {
                        if let Err(e) = engine.delete(&id) {
                            warn!("Failed to delete record! {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get record ID! {}", e);
                    }
                    _ => {}
                }
            }
            'U' | 'u' => {
                info!("Updating record fields");
                match prompt_record_id(&engine, "update") {
                    Err(e) => {
                        warn!("Failed to get record ID! {}", e);
                    }
                    Ok(Some(id)) => {
                        let mut login = match engine.get(&id) {
                            Ok(Some(login)) => login,
                            Ok(None) => {
                                warn!("No such login!");
                                continue
                            }
                            Err(e) => {
                                warn!("Failed to update record (get failed) {}", e);
                                continue;
                            }
                        };
                        update_login(&mut login);
                        if let Err(e) = engine.update(login) {
                            warn!("Failed to update record! {}", e);
                        }
                    }
                    _ => {}
                }
            }
            'R' | 'r' => {
                info!("Resetting client.");
                if let Err(e) = engine.db.reset() {
                    warn!("Failed to reset! {}", e);
                }
            }
            'W' | 'w' => {
                info!("Wiping all data from client!");
                if let Err(e) = engine.db.wipe() {
                    warn!("Failed to wipe! {}", e);
                }
            }
            'S' | 's' => {
                info!("Syncing!");
                if let Err(e) = engine.sync(&client_init, &root_sync_key) {
                    warn!("Sync failed! {}", e);
                    warn!("BT: {:?}", e.backtrace());
                } else {
                    info!("Sync was successful!");
                }
            }
            'V' | 'v' => {
                if let Err(e) = show_all(&engine) {
                    warn!("Failed to dump passwords? This is probably bad! {}", e);
                }
            }
            'T' | 't' => {
                info!("Touching (bumping use count) for a record");
                match prompt_record_id(&engine, "update") {
                    Err(e) => {
                        warn!("Failed to get record ID! {}", e);
                    }
                    Ok(Some(id)) => {
                        if let Err(e) = engine.touch(&id) {
                            warn!("Failed to touch record! {}", e);
                        }
                    }
                    _ => {}
                }
            }
            'x' | 'X' => {
                info!("Running arbitrary SQL, there's no way this could go wrong!");
                if let Some(sql) = prompt_string("SQL (one line only, press enter when done):\n") {
                    if let Err(e) = show_sql(&engine, &sql) {
                        warn!("Failed to run sql query: {}", e);
                    }
                }
            }
            'Q' | 'q' => {
                break;
            }
            '?' => {
                continue;
            }
            c => {
                println!("Unknown action '{}', exiting.", c);
                break;
            }
        }
    }

    println!("Exiting (bye!)");
    Ok(())
}
