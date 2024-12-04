/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![recursion_limit = "4096"]
#![warn(rust_2018_idioms)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cli_support::fxa_creds::{
    get_account_and_token, get_cli_fxa, get_default_fxa_config, SYNC_SCOPE,
};
use cli_support::prompt::{prompt_char, prompt_string, prompt_usize};
use logins::encryption::{create_key, ManagedEncryptorDecryptor, StaticKeyManager};
use logins::{
    Login, LoginEntry, LoginFields, LoginStore, LoginsSyncEngine, SecureLoginFields,
    ValidateAndFixup,
};

use prettytable::{row, Cell, Row, Table};
use std::fs;
use std::sync::Arc;
use sync15::{
    client::{sync_multiple, MemoryCachedState, Sync15StorageClientInit},
    engine::{EngineSyncAssociation, SyncEngine},
};

// I'm completely punting on good error handling here.
use anyhow::Result;

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
        fields: LoginFields {
            username_field,
            password_field,
            form_action_origin,
            http_realm: None,
            origin,
        },
        sec_fields: SecureLoginFields { username, password },
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
        fields: LoginFields {
            username_field,
            password_field,
            form_action_origin: None,
            http_realm,
            origin,
        },
        sec_fields: SecureLoginFields { username, password },
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

fn update_encrypted_fields(fields: &mut SecureLoginFields, extra: &str) {
    if let Some(v) = prompt_string(format!("new username [now {}{}]", fields.username, extra)) {
        fields.username = v;
    };
    if let Some(v) = prompt_string(format!("new password [now {}{}]", fields.password, extra)) {
        fields.password = v;
    };
}

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(AsRef::as_ref)
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn update_login(login: Login) -> LoginEntry {
    let mut record = LoginEntry {
        sec_fields: login.sec_fields,
        fields: login.fields,
    };
    update_encrypted_fields(&mut record.sec_fields, ", leave blank to keep");
    update_string("origin", &mut record.fields.origin, ", leave blank to keep");

    update_string(
        "username_field",
        &mut record.fields.username_field,
        ", leave blank to keep",
    );
    update_string(
        "password_field",
        &mut record.fields.password_field,
        ", leave blank to keep",
    );

    if prompt_bool(&format!(
        "edit form_action_origin? (now {}) [yN]",
        string_opt_or(&record.fields.form_action_origin, "(none)")
    ))
    .unwrap_or(false)
    {
        record.fields.form_action_origin = prompt_string("form_action_origin");
    }

    if prompt_bool(&format!(
        "edit http_realm? (now {}) [yN]",
        string_opt_or(&record.fields.http_realm, "(none)")
    ))
    .unwrap_or(false)
    {
        record.fields.http_realm = prompt_string("http_realm");
    }

    if let Err(e) = record.check_valid() {
        log::warn!("Warning: produced invalid record: {}", e);
        // but we return it anyway!
    }
    record
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
            .map(|name| Cell::new(name).style_spec("bc"))
            .collect(),
    ));

    let rows = stmt.query_map([], |row| {
        (0..len)
            .map(|idx| {
                Ok(match row.get::<_, Value>(idx)? {
                    Value::Null => Cell::new("null").style_spec("Fd"),
                    Value::Integer(i) => Cell::new(&i.to_string()).style_spec("Fb"),
                    Value::Real(r) => Cell::new(&r.to_string()).style_spec("Fb"),
                    Value::Text(s) => Cell::new(&s).style_spec("Fr"),
                    Value::Blob(b) => Cell::new(&format!("{}b blob", b.len())),
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

fn show_all(store: &LoginStore) -> Result<Vec<String>> {
    let logins = store.list()?;

    let mut table = prettytable::Table::new();

    table.add_row(row![bc =>
        "(idx)",
        "Guid",
        "Username",
        "Password",
        "Origin",

        "Action Origin",
        "HTTP Realm",

        "User Field",
        "Pass Field",

        "Uses",
        "Created At",
        "Changed At",
        "Last Used"
    ]);

    let mut v = Vec::with_capacity(logins.len());
    let mut logins_copy = logins.clone();
    logins_copy.sort_by_key(|a| a.guid());
    for login in logins.iter() {
        table.add_row(row![
            r->v.len(),
            Fr->&login.guid(),
            &login.sec_fields.username,
            &login.sec_fields.password,
            &login.fields.origin,

            string_opt_or(&login.fields.form_action_origin, ""),
            string_opt_or(&login.fields.http_realm, ""),

            &login.fields.username_field,
            &login.fields.password_field,

            login.record.times_used,
            timestamp_to_string(login.record.time_created),
            timestamp_to_string(login.record.time_password_changed),
            if login.record.time_last_used == 0 {
                "Never".to_owned()
            } else {
                timestamp_to_string(login.record.time_last_used)
            }
        ]);
        v.push(login.guid().to_string());
    }
    table.printstd();
    Ok(v)
}

fn prompt_record_id(s: &LoginStore, action: &str) -> Result<Option<String>> {
    let index_to_id = show_all(s)?;
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

fn open_database(db_path: &str) -> Result<(LoginStore, String)> {
    let encryption_key = get_or_create_encryption_key()?;
    let encdec = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(
        StaticKeyManager::new(encryption_key.clone()),
    )));
    let store = LoginStore::new(db_path, encdec)?;
    Ok((store, encryption_key))
}

fn get_or_create_encryption_key() -> Result<String> {
    match get_encryption_key() {
        Ok(encryption_key) => Ok(encryption_key),
        Err(_) => {
            let encryption_key = create_key()?;
            set_encryption_key(encryption_key.clone())?;
            Ok(encryption_key)
        }
    }
}

fn get_encryption_key() -> Result<String, std::io::Error> {
    fs::read_to_string("logins.jwk")
}

fn set_encryption_key(encryption_key: String) -> Result<(), std::io::Error> {
    fs::write("logins.jwk", encryption_key)
}

fn do_sync(
    store: Arc<LoginStore>,
    key_id: String,
    access_token: String,
    sync_key: String,
    tokenserver_url: url::Url,
    local_encryption_key: String,
) -> Result<String> {
    let mut engine = LoginsSyncEngine::new(Arc::clone(&store))?;
    engine
        .set_local_encryption_key(&local_encryption_key)
        .unwrap();

    let storage_init = &Sync15StorageClientInit {
        key_id,
        access_token,
        tokenserver_url,
    };
    let root_sync_key = &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?;

    let mut disk_cached_state = engine.get_global_state()?;
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
    engine.set_global_state(&disk_cached_state)?;

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
    viaduct_reqwest::use_reqwest_backend();

    let matches = clap::Command::new("sync_pass_sql")
        .about("CLI login syncing tool")
        .arg(
            clap::Arg::new("database_path")
                .short('d')
                .long("database")
                .default_value("./logins.db")
                .value_name("LOGINS_DATABASE")
                .num_args(1)
                .help("Path to the logins database (default: \"./logins.db\")"),
        )
        .arg(
            clap::Arg::new("credential_file")
                .short('c')
                .long("credentials")
                .default_value("./credentials.json")
                .value_name("CREDENTIAL_JSON")
                .num_args(1)
                .help(
                    "Path to store our cached fxa credentials (defaults to \"./credentials.json\"",
                ),
        )
        .get_matches();

    let cred_file = matches.get_one::<String>("credential_file").unwrap();
    let db_path = matches.get_one::<String>("database_path").unwrap();

    log::debug!("credential file: {:?}", cred_file);
    log::debug!("db: {:?}", db_path);
    // Lets not log the encryption key, it's just not a good habit to be in.

    let (store, encryption_key) = open_database(db_path)?;
    let store = Arc::new(store);

    log::info!("Store has {} passwords", store.list()?.len());

    if let Err(e) = show_all(&store) {
        log::warn!("Failed to show initial login data! {}", e);
    }

    loop {
        match prompt_char("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew, [B]ase-domain search, [R]eset, [W]ipe, [T]ouch, E[x]ecute SQL Query, or [Q]uit").unwrap_or('?') {
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
                let (_, token_info) = get_account_and_token(get_default_fxa_config(), cred_file, &[SYNC_SCOPE])?;
                let sync_key = URL_SAFE_NO_PAD.encode(
                    token_info.key.unwrap().key_bytes()?,
                );
                // TODO: allow users to use stage/etc.
                let cli_fxa = get_cli_fxa(get_default_fxa_config(), cred_file, &[SYNC_SCOPE])?;
                match do_sync(
                    Arc::clone(&store),
                    cli_fxa.client_init.key_id.clone(),
                    cli_fxa.client_init.access_token.clone(),
                    sync_key,
                    cli_fxa.client_init.tokenserver_url.clone(),
                    encryption_key.clone(),
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
                if let Err(e) = show_all(&store) {
                    log::warn!("Failed to dump passwords? This is probably bad! {}", e);
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
                    let db = store.db.lock();
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
