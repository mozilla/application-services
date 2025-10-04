/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![recursion_limit = "4096"]
#![warn(rust_2018_idioms)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cli_support::fxa_creds::{
    get_account_and_token, get_cli_fxa, get_default_fxa_config, SYNC_SCOPE,
};
use cli_support::prompt::{prompt_char, prompt_password, prompt_string, prompt_usize};
use logins::encryption::{ManagedEncryptorDecryptor, NSSKeyManager, PrimaryPasswordAuthenticator};
use logins::{Login, LoginEntry, LoginStore, LoginsApiError, LoginsSyncEngine, ValidateAndFixup};

use async_trait::async_trait;
use std::sync::Arc;
use sync15::{
    client::{sync_multiple, MemoryCachedState, Sync15StorageClientInit},
    engine::{EngineSyncAssociation, SyncEngine},
};
use text_table::{row, Cell, Row, Table};

// I'm completely punting on good error handling here.
use anyhow::Result;

enum PasswordVisibility {
    Reveal,
    Hide,
}

fn read_login() -> LoginEntry {
    let login = loop {
        match prompt_char("Choose login kind: [F]orm based, [A]uth based").unwrap() {
            'F' | 'f' => {
                break read_form_based_login();
            }
            'A' | 'a' => {
                break read_auth_based_login();
            }
            c => {
                println!("Unknown choice '{}', exiting.", c);
            }
        }
    };

    if let Err(e) = login.check_valid() {
        log::warn!("Warning: produced invalid record: {}", e);
    }
    login
}

fn read_form_based_login() -> LoginEntry {
    let username = prompt_string("username").unwrap_or_default();
    let password = prompt_string("password").unwrap_or_default();
    let form_action_origin = prompt_string("form_action_origin (example: https://www.example.com)");
    let origin = prompt_string("origin (example: https://www.example.com)").unwrap_or_default();
    let username_field = prompt_string("username_field").unwrap_or_default();
    let password_field = prompt_string("password_field").unwrap_or_default();
    LoginEntry {
        username_field,
        password_field,
        form_action_origin,
        http_realm: None,
        origin,
        username,
        password,
    }
}

fn read_auth_based_login() -> LoginEntry {
    let username = prompt_string("username").unwrap_or_default();
    let password = prompt_string("password").unwrap_or_default();
    let origin = prompt_string("origin (example: https://www.example.com)").unwrap_or_default();
    let http_realm = prompt_string("http_realm (example: My Auth Realm)");
    let username_field = prompt_string("username_field").unwrap_or_default();
    let password_field = prompt_string("password_field").unwrap_or_default();
    LoginEntry {
        username_field,
        password_field,
        form_action_origin: None,
        http_realm,
        origin,
        username,
        password,
    }
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

fn update_username_and_password(entry: &mut LoginEntry, extra: &str) {
    if let Some(v) = prompt_string(format!("new username [now {}{}]", entry.username, extra)) {
        entry.username = v;
    };
    if let Some(v) = prompt_string(format!("new password [now {}{}]", entry.password, extra)) {
        entry.password = v;
    };
}

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(AsRef::as_ref)
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn update_login(login: Login) -> LoginEntry {
    let mut entry = login.entry();
    update_username_and_password(&mut entry, ", leave blank to keep");
    update_string("origin", &mut entry.origin, ", leave blank to keep");

    update_string(
        "username_field",
        &mut entry.username_field,
        ", leave blank to keep",
    );
    update_string(
        "password_field",
        &mut entry.password_field,
        ", leave blank to keep",
    );

    if prompt_bool(&format!(
        "edit form_action_origin? (now {}) [yN]",
        string_opt_or(&entry.form_action_origin, "(none)")
    ))
    .unwrap_or(false)
    {
        entry.form_action_origin = prompt_string("form_action_origin");
    }

    if prompt_bool(&format!(
        "edit http_realm? (now {}) [yN]",
        string_opt_or(&entry.http_realm, "(none)")
    ))
    .unwrap_or(false)
    {
        entry.http_realm = prompt_string("http_realm");
    }

    if let Err(e) = entry.check_valid() {
        log::warn!("Warning: produced invalid record: {}", e);
        // but we return it anyway!
    }
    entry
}

fn prompt_bool(msg: &str) -> Option<bool> {
    let result = prompt_string(msg);
    result.and_then(|r| match r.chars().next().unwrap() {
        'y' | 'Y' | 't' | 'T' => Some(true),
        'n' | 'N' | 'f' | 'F' => Some(false),
        _ => None,
    })
}

fn timestamp_to_string(milliseconds: i64) -> String {
    use chrono::{DateTime, Local};
    use std::time::{Duration, UNIX_EPOCH};
    let time = UNIX_EPOCH + Duration::from_millis(milliseconds as u64);
    let dtl: DateTime<Local> = time.into();
    dtl.format("%l:%M:%S %p%n%h %e, %Y").to_string()
}

fn show_sql(conn: &rusqlite::Connection, sql: &str) -> Result<()> {
    use rusqlite::types::Value;
    let mut stmt = conn.prepare(sql)?;
    let cols: Vec<String> = stmt
        .column_names()
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    let len = cols.len();
    let mut table = Table::new();
    table.add_row(Row::new(
        cols.iter()
            .map(|name| Cell::new(name).align_center())
            .collect(),
    ));

    let rows = stmt.query_map([], |row| {
        (0..len)
            .map(|idx| {
                Ok(match row.get::<_, Value>(idx)? {
                    Value::Null => Cell::new("null"),
                    Value::Integer(i) => Cell::new(i.to_string()),
                    Value::Real(r) => Cell::new(r.to_string()),
                    Value::Text(s) => Cell::new(&s).align_right(),
                    Value::Blob(b) => Cell::new(format!("{}b blob", b.len())),
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;

    for row in rows {
        table.add_row(Row::new(row?));
    }
    table.printstd();
    Ok(())
}

fn show_login(
    store: &LoginStore,
    target_login: &String,
    show_password: PasswordVisibility,
) -> Result<Vec<String>> {
    let logins = store.list()?;
    let mut table = Table::new();
    let mut v = Vec::new();

    let index = logins
        .iter()
        .position(|login| &login.id == target_login)
        .unwrap();
    let login = &logins[index];

    table.add_row(row!["(idx)", index]);
    table.add_row(row!["Guid", login.guid()]);
    table.add_row(row!["Username", login.username]);
    let password = match show_password {
        PasswordVisibility::Hide => "*".repeat(login.password.len()),
        PasswordVisibility::Reveal => login.password.clone(),
    };
    table.add_row(row!["Password", password]);
    table.add_row(row!["Origin", login.origin]);
    table.add_row(row![
        "Action Origin",
        string_opt_or(&login.form_action_origin, "")
    ]);
    table.add_row(row!["HTTP Realm", string_opt_or(&login.http_realm, "")]);
    table.add_row(row!["User Field", login.username_field]);
    table.add_row(row!["Pass Field", login.password_field]);
    table.add_row(row!["Uses", login.times_used]);
    table.add_row(row!["Created At", timestamp_to_string(login.time_created)]);
    table.add_row(row![
        "Changed At",
        timestamp_to_string(login.time_password_changed)
    ]);
    let last_used = if login.time_last_used == 0 {
        "Never".to_owned()
    } else {
        timestamp_to_string(login.time_last_used)
    };
    table.add_row(row!["Last Used", last_used]);
    v.push(login.guid().to_string());

    table.printstd();
    Ok(v)
}

fn show_logins(store: &LoginStore) -> Result<Vec<String>> {
    let logins = store.list()?;
    let mut table = Table::new();
    let row = Row::default()
        .add_center("(idx)")
        .add_cell("Origin")
        .add_cell("Username");

    table.add_row(row);

    let mut v = Vec::with_capacity(logins.len());

    for login in logins.iter() {
        let row = Row::default()
            .add_right(v.len())
            .add_cell(&login.origin)
            .add_cell(&login.username);

        table.add_row(row);
        v.push(login.guid().to_string());
    }
    table.printstd();
    Ok(v)
}

fn prompt_record_id(s: &LoginStore, action: &str) -> Result<Option<String>> {
    let index_to_id = show_logins(s)?;
    let input = if let Some(input) = prompt_usize(format!("Enter (idx) of record to {}", action)) {
        input
    } else {
        return Ok(None);
    };
    if input >= index_to_id.len() {
        log::info!("No such index");
        return Ok(None);
    }
    Ok(Some(index_to_id[input].as_str().into()))
}

struct MyPrimaryPasswordAuthenticator {}
#[async_trait]
impl PrimaryPasswordAuthenticator for MyPrimaryPasswordAuthenticator {
    async fn get_primary_password(&self) -> Result<String, LoginsApiError> {
        let password = prompt_password("primary password").unwrap_or_default();
        Ok(password)
    }

    async fn on_authentication_success(&self) -> Result<(), LoginsApiError> {
        println!("success");
        Ok(())
    }

    async fn on_authentication_failure(&self) -> Result<(), LoginsApiError> {
        println!("this did not work, please try again:");
        Ok(())
    }
}

fn open_database(db_path: &str) -> Result<LoginStore> {
    let key_manager = NSSKeyManager::new(Arc::new(MyPrimaryPasswordAuthenticator {}));
    let encdec = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(key_manager)));
    let store = LoginStore::new(db_path, encdec)?;
    Ok(store)
}

fn do_sync(
    store: Arc<LoginStore>,
    key_id: String,
    access_token: String,
    sync_key: String,
    tokenserver_url: url::Url,
) -> Result<String> {
    let engine = LoginsSyncEngine::new(Arc::clone(&store))?;

    let storage_init = &Sync15StorageClientInit {
        key_id,
        access_token,
        tokenserver_url,
    };
    let root_sync_key = &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?;

    // We don't track any state at all - this means every sync acts like a first sync.
    // We should consider supporting this - choices would be to re-open the database ourself and abuse the
    // meta tables, storing a disk on file, etc.
    let mut disk_cached_state = None;
    let mut mem_cached_state = MemoryCachedState::default();

    let mut result = sync_multiple(
        &[&engine],
        &mut disk_cached_state,
        &mut mem_cached_state,
        storage_init,
        root_sync_key,
        &engine.scope,
        None,
    );

    // and here we should persist `disk_cached_state` somewhere.

    if let Err(e) = result.result {
        return Err(e.into());
    }
    match result.engine_results.remove("passwords") {
        None | Some(Ok(())) => Ok(serde_json::to_string(&result.telemetry).unwrap()),
        Some(Err(e)) => Err(e.into()),
    }
}

#[allow(clippy::cognitive_complexity)] // FIXME
fn main() -> Result<()> {
    cli_support::init_trace_logging();
    viaduct_hyper::viaduct_init_backend_hyper().expect("Error initializing viaduct");

    let matches = clap::Command::new("sync_pass_sql")
        .about("CLI login syncing tool")
        .arg(
            clap::Arg::new("profile_path")
                .short('p')
                .long("profile")
                .default_value("./")
                .value_name("PROFILE_PATH")
                .num_args(1)
                .help("Path to the profile directory (default: \"./\")"),
        )
        .arg(
            clap::Arg::new("database_path")
                .short('d')
                .long("database")
                .default_value("./logins.db")
                .default_value_if("profile_path", clap::builder::ArgPredicate::IsPresent, None)
                .value_name("LOGINS_DATABASE")
                .num_args(1)
                .help("Path to the logins database (default: \"./logins.db\")"),
        )
        .arg(
            clap::Arg::new("credential_file")
                .short('c')
                .long("credentials")
                .default_value("./credentials.json")
                .default_value_if("profile_path", clap::builder::ArgPredicate::IsPresent, None)
                .value_name("CREDENTIAL_JSON")
                .num_args(1)
                .help(
                    "Path to store our cached fxa credentials (defaults to \"./credentials.json\"",
                ),
        )
        .get_matches();

    let profile_path = matches.get_one::<String>("profile_path").unwrap();
    let cred_file = matches
        .get_one::<String>("credential_file")
        .cloned()
        .unwrap_or_else(|| {
            std::path::Path::new(profile_path)
                .join("credentials.json")
                .display()
                .to_string()
        });
    let db_path = matches
        .get_one::<String>("database_path")
        .cloned()
        .unwrap_or_else(|| {
            std::path::Path::new(profile_path)
                .join("logins.db")
                .display()
                .to_string()
        });

    log::debug!("credential file: {:?}", cred_file);
    log::debug!("db: {:?}", db_path);
    log::debug!("profile: {:?}", profile_path);

    init_rust_components::initialize(profile_path.to_string());

    let store = Arc::new(open_database(&db_path)?);

    log::info!("Store has {} passwords", store.list()?.len());

    if let Err(e) = show_logins(&store) {
        log::warn!("Failed to show initial login data! {}", e);
    }

    loop {
        match prompt_char("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew All, [E]xamine, [B]ase-domain search, [R]eset, [W]ipe, [T]ouch, E[x]ecute SQL Query, or [Q]uit").unwrap_or('?') {
            'A' | 'a' => {
                log::info!("Adding new record");
                let record = read_login();
                if let Err(e) = store.add(record) {
                    log::warn!("Failed to create record! {}", e);
                }
            }
            'D' | 'd' => {
                log::info!("Deleting record");
                match prompt_record_id(&store, "delete") {
                    Ok(Some(id)) => {
                        if let Err(e) = store.delete(&id) {
                            log::warn!("Failed to delete record! {}", e);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to get record ID! {}", e);
                    }
                    _ => {}
                }
        }
            'U' | 'u' => {
                log::info!("Updating record fields");
                match prompt_record_id(&store, "update") {
                    Err(e) => {
                        log::warn!("Failed to get record ID! {}", e);
                    }
                    Ok(Some(id)) => {
                        let login_record = match store.get(&id) {
                            Ok(Some(login_record)) => login_record,
                            Ok(None) => {
                                log::warn!("No such login!");
                                continue
                            }
                            Err(e) => {
                                log::warn!("Failed to update record (get failed) {}", e);
                                continue;
                            }
                        };
                        if let Err(e) = store.update(&id, update_login(login_record.clone())) {
                            log::warn!("Failed to update record! {}", e);
                        }
                    }
                    _ => {}
                }
            }
            'R' | 'r' => {
                log::info!("Resetting client.");
                let engine = LoginsSyncEngine::new(Arc::clone(&store))?;
                if let Err(e) = engine.reset(&EngineSyncAssociation::Disconnected) {
                    log::warn!("Failed to reset! {}", e);
                }
            }
            'S' | 's' => {
                log::info!("Syncing!");
                let (_, token_info) = get_account_and_token(get_default_fxa_config(), &cred_file, &[SYNC_SCOPE])?;
                let sync_key = URL_SAFE_NO_PAD.encode(
                    token_info.key.unwrap().key_bytes()?,
                );
                // TODO: allow users to use stage/etc.
                let cli_fxa = get_cli_fxa(get_default_fxa_config(), &cred_file, &[SYNC_SCOPE])?;
                match do_sync(
                    Arc::clone(&store),
                    cli_fxa.client_init.key_id.clone(),
                    cli_fxa.client_init.access_token.clone(),
                    sync_key,
                    cli_fxa.client_init.tokenserver_url.clone(),
                ) {
                    Err(e) => {
                        log::warn!("Sync failed! {}", e);
                    },
                    Ok(sync_ping) => {
                        log::info!("Sync was successful!");
                        log::info!("Sync telemetry: {}", serde_json::to_string_pretty(&sync_ping).unwrap());
                    }
                }
            }
            'V' | 'v' => {
                if let Err(e) = show_logins(&store) {
                    log::warn!("Failed to dump passwords? This is probably bad! {}", e);
                }
            }
            'E' | 'e' => {
                match prompt_record_id(&store, "examine") {
                    Ok(Some(id)) => {
                        let password_visibility = match prompt_char("Would you like to reveal your password? [Y]es/[N]o") {
                            Some(result) => {
                                match result {
                                    'Y' | 'y' => PasswordVisibility::Reveal,
                                    'N' | 'n' => PasswordVisibility::Hide,
                                    _ => PasswordVisibility::Hide,
                                }
                            }
                            None => PasswordVisibility::Hide
                        };
                        if let Err(e) = show_login(&store, &id, password_visibility) {
                            log::warn!("Failed to dump passwords? This is probably bad! {}", e);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to get record ID! {}", e);
                    }
                    _ => {}
                }
            }
            'B' | 'b' => {
                log::info!("Base Domain search");
                if let Some(basedomain) = prompt_string("Base domain (one line only, press enter when done):\n") {
                    match store.get_by_base_domain(&basedomain) {
                        Err(e) => {
                            log::warn!("Base domain lookup failed! {}", e);
                        },
                        Ok(result) => {
                            log::info!("Base domain result: {:?}", result);
                        }
                    }
                }
            }
            'T' | 't' => {
                log::info!("Touching (bumping use count) for a record");
                match prompt_record_id(&store, "update") {
                    Err(e) => {
                        log::warn!("Failed to get record ID! {}", e);
                    }
                    Ok(Some(id)) => {
                        if let Err(e) = store.touch(&id) {
                            log::warn!("Failed to touch record! {}", e);
                        }
                    }
                    _ => {}
                }
            }
            'x' | 'X' => {
                log::info!("Running arbitrary SQL, there's no way this could go wrong!");
                if let Some(sql) = prompt_string("SQL (one line only, press enter when done):\n") {
                    let db = store.lock_db()?;
                    if let Err(e) = show_sql(&db, &sql) {
                        log::warn!("Failed to run sql query: {}", e);
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
