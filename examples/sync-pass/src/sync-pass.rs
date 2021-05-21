/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![recursion_limit = "4096"]
#![warn(rust_2018_idioms)]

use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config};
use cli_support::prompt::{prompt_char, prompt_string, prompt_usize};
use logins::encryption::{create_key, EncryptorDecryptor};
use logins::{Login, LoginsSyncEngine, PasswordStore};
use prettytable::{cell, row, Cell, Row, Table};
use rusqlite::{OptionalExtension, NO_PARAMS};
use sync15::{EngineSyncAssociation, SyncEngine};
use sync_guid::Guid;

// I'm completely punting on good error handling here.
use anyhow::{bail, Result};

fn read_login(encdec: &EncryptorDecryptor) -> Login {
    let login = loop {
        match prompt_char("Choose login kind: [F]orm based, [A]uth based").unwrap() {
            'F' | 'f' => {
                break read_form_based_login(encdec);
            }
            'A' | 'a' => {
                break read_auth_based_login(encdec);
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

fn read_form_based_login(encdec: &EncryptorDecryptor) -> Login {
    let username = prompt_string("username").unwrap_or_default();
    let password = prompt_string("password").unwrap_or_default();
    let form_submit_url = prompt_string("form_submit_url (example: https://www.example.com)");
    let hostname = prompt_string("hostname (example: https://www.example.com)").unwrap_or_default();
    let username_field = prompt_string("username_field").unwrap_or_default();
    let password_field = prompt_string("password_field").unwrap_or_default();
    Login {
        guid: Guid::random(),
        username_enc: encdec.encrypt(&username).unwrap(),
        password_enc: encdec.encrypt(&password).unwrap(),
        username_field,
        password_field,
        form_submit_url,
        http_realm: None,
        hostname,
        ..Login::default()
    }
}

fn read_auth_based_login(encdec: &EncryptorDecryptor) -> Login {
    let username = prompt_string("username").unwrap_or_default();
    let password = prompt_string("password").unwrap_or_default();
    let hostname = prompt_string("hostname (example: https://www.example.com)").unwrap_or_default();
    let http_realm = prompt_string("http_realm (example: My Auth Realm)");
    let username_field = prompt_string("username_field").unwrap_or_default();
    let password_field = prompt_string("password_field").unwrap_or_default();
    Login {
        guid: Guid::random(),
        username_enc: encdec.encrypt(&username).unwrap(),
        password_enc: encdec.encrypt(&password).unwrap(),
        username_field,
        password_field,
        form_submit_url: None,
        http_realm,
        hostname,
        ..Login::default()
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

fn update_encrypted_string(
    field_name: &str,
    field: &mut String,
    extra: &str,
    encdec: &EncryptorDecryptor,
) -> bool {
    let opt_s = prompt_string(format!(
        "new {} [now {}{}]",
        field_name,
        encdec.decrypt(field).unwrap(),
        extra
    ));
    if let Some(s) = opt_s {
        *field = encdec.encrypt(&s).unwrap();
        true
    } else {
        false
    }
}

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(AsRef::as_ref)
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn update_login(record: &mut Login, encdec: &EncryptorDecryptor) {
    update_encrypted_string(
        "username",
        &mut record.username_enc,
        ", leave blank to keep",
        encdec,
    );
    update_encrypted_string(
        "password",
        &mut record.password_enc,
        ", leave blank to keep",
        encdec,
    );
    update_string("hostname", &mut record.hostname, ", leave blank to keep");

    update_string(
        "username_field",
        &mut record.username_field,
        ", leave blank to keep",
    );
    update_string(
        "password_field",
        &mut record.password_field,
        ", leave blank to keep",
    );

    if prompt_bool(&format!(
        "edit form_submit_url? (now {}) [yN]",
        string_opt_or(&record.form_submit_url, "(none)")
    ))
    .unwrap_or(false)
    {
        record.form_submit_url = prompt_string("form_submit_url");
    }

    if prompt_bool(&format!(
        "edit http_realm? (now {}) [yN]",
        string_opt_or(&record.http_realm, "(none)")
    ))
    .unwrap_or(false)
    {
        record.http_realm = prompt_string("http_realm");
    }

    if let Err(e) = record.check_valid() {
        log::warn!("Warning: produced invalid record: {}", e);
    }
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

fn show_sql(s: &PasswordStore, sql: &str) -> Result<()> {
    use rusqlite::types::Value;
    let conn = &s.db;
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
            .map(|name| Cell::new(&name).style_spec("bc"))
            .collect(),
    ));

    let rows = stmt.query_map(NO_PARAMS, |row| {
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

fn show_all(store: &PasswordStore, encdec: &EncryptorDecryptor) -> Result<Vec<Guid>> {
    let records = store.list()?;

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
    record_copy.sort_by(|a, b| a.guid.cmp(&b.guid));
    for rec in records.iter() {
        table.add_row(row![
            r->v.len(),
            Fr->&rec.guid,
            &encdec.decrypt(&rec.username_enc).unwrap(),
            Fb->&encdec.decrypt(&rec.password_enc).unwrap(),

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
        v.push(rec.guid.clone());
    }
    table.printstd();
    Ok(v)
}

fn prompt_record_id(
    s: &PasswordStore,
    encdec: &EncryptorDecryptor,
    action: &str,
) -> Result<Option<String>> {
    let index_to_id = show_all(s, encdec)?;
    let input = if let Some(input) = prompt_usize(&format!("Enter (idx) of record to {}", action)) {
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

fn open_database(
    db_path: &str,
    sqlcipher_path: Option<&str>,
    sqlcipher_encryption_key: Option<&str>,
) -> Result<(PasswordStore, EncryptorDecryptor, String)> {
    Ok(match (sqlcipher_path, sqlcipher_encryption_key) {
        (None, None) => {
            let store = PasswordStore::new(db_path)?;
            // Get or create an encryption key to use
            let encryption_key = match get_encryption_key(&store) {
                Some(s) => s,
                None => {
                    log::warn!("Creating new encryption key");
                    let encryption_key = create_key()?;
                    set_encryption_key(&store, &encryption_key)?;
                    encryption_key
                }
            };
            (
                store,
                EncryptorDecryptor::new(&encryption_key)?,
                encryption_key,
            )
        }
        (Some(sqlcipher_path), Some(sqlcipher_encryption_key)) => {
            let encryption_key = create_key()?;
            let store = PasswordStore::new_with_sqlcipher_migration(
                db_path,
                &encryption_key,
                sqlcipher_path,
                sqlcipher_encryption_key,
                None,
            )?;
            // For new migrations, we want to set the encryption key.  But it's also possible that
            // the migration already happened, in that use the encryption key from that migration.
            let encryption_key = match get_encryption_key(&store) {
                Some(s) => s,
                None => {
                    set_encryption_key(&store, &encryption_key)?;
                    encryption_key
                }
            };
            (
                store,
                EncryptorDecryptor::new(&encryption_key)?,
                encryption_key,
            )
        }
        _ => {
            bail!("--sqlcipher-database and --sqlcipher-key must be specified together");
        }
    })
}

// Use loginsSyncMeta as a quick and dirty solution to store the encryption key
fn get_encryption_key(store: &PasswordStore) -> Option<String> {
    store
        .db
        .query_row(
            "SELECT value FROM loginsSyncMeta WHERE key = 'sync-pass-key'",
            NO_PARAMS,
            |r| r.get(0),
        )
        .optional()
        .unwrap()
}

fn set_encryption_key(store: &PasswordStore, key: &str) -> rusqlite::Result<()> {
    store
        .db
        .execute(
            "
        INSERT INTO  loginsSyncMeta (key, value)
        VALUES ('sync-pass-key', ?)
        ",
            &[&key],
        )
        .map(|_| ())
}

#[allow(clippy::cognitive_complexity)] // FIXME
fn main() -> Result<()> {
    cli_support::init_trace_logging();
    viaduct_reqwest::use_reqwest_backend();
    std::env::set_var("RUST_BACKTRACE", "1");

    let matches = clap::App::new("sync_pass_sql")
        .about("CLI login syncing tool (backed by sqlcipher)")
        .arg(
            clap::Arg::with_name("database_path")
                .short("d")
                .long("database")
                .value_name("LOGINS_DATABASE")
                .takes_value(true)
                .help("Path to the logins database (default: \"./logins.db\")"),
        )
        .arg(
            clap::Arg::with_name("credential_file")
                .short("c")
                .long("credentials")
                .value_name("CREDENTIAL_JSON")
                .takes_value(true)
                .help(
                    "Path to store our cached fxa credentials (defaults to \"./credentials.json\"",
                ),
        )
        .arg(
            clap::Arg::with_name("sqlcipher_database_path")
                .long("sqlcipher-database")
                .value_name("SQLCIPHER_DATABASE")
                .takes_value(true)
                .help("Path to a sqlcipher database to migrate"),
        )
        .arg(
            clap::Arg::with_name("sqlcipher_key")
                .long("sqlcipher-key")
                .value_name("SQLCIPHER_KEY")
                .takes_value(true)
                .help("Encryption key for the sql cipher database"),
        )
        .get_matches();

    let cred_file = matches
        .value_of("credential_file")
        .unwrap_or("./credentials.json");
    let db_path = matches.value_of("database_path").unwrap_or("./logins.db");
    let sqlcipher_database_path = matches.value_of("sqlcipher_database_path");
    let sqlcipher_key = matches.value_of("sqlcipher_key");

    log::debug!("credential file: {:?}", cred_file);
    log::debug!("db: {:?}", db_path);
    log::debug!("sqlcipher_database_path: {:?}", sqlcipher_database_path);
    log::debug!("sqlcipher_key: {:?}", sqlcipher_key);
    // Lets not log the encryption key, it's just not a good habit to be in.

    // TODO: allow users to use stage/etc.
    let cli_fxa = get_cli_fxa(get_default_fxa_config(), cred_file)?;
    let (store, encdec, encryption_key) =
        open_database(db_path, sqlcipher_database_path, sqlcipher_key)?;

    log::info!("Store has {} passwords", store.list()?.len());

    if let Err(e) = show_all(&store, &encdec) {
        log::warn!("Failed to show initial login data! {}", e);
    }

    loop {
        match prompt_char("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew, [B]ase-domain search, [R]eset, [W]ipe, [T]ouch, E[x]ecute SQL Query, or [Q]uit").unwrap_or('?') {
            'A' | 'a' => {
                log::info!("Adding new record");
                let record = read_login(&encdec);
                if let Err(e) = store.add(record) {
                    log::warn!("Failed to create record! {}", e);
                }
            }
            'D' | 'd' => {
                log::info!("Deleting record");
                match prompt_record_id(&store, &encdec, "delete") {
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
                match prompt_record_id(&store, &encdec, "update") {
                    Err(e) => {
                        log::warn!("Failed to get record ID! {}", e);
                    }
                    Ok(Some(id)) => {
                        let mut login = match store.get(&id) {
                            Ok(Some(login)) => login,
                            Ok(None) => {
                                log::warn!("No such login!");
                                continue
                            }
                            Err(e) => {
                                log::warn!("Failed to update record (get failed) {}", e);
                                continue;
                            }
                        };
                        update_login(&mut login, &encdec);
                        if let Err(e) = store.update(login) {
                            log::warn!("Failed to update record! {}", e);
                        }
                    }
                    _ => {}
                }
            }
            'R' | 'r' => {
                log::info!("Resetting client.");
                let engine = LoginsSyncEngine::new(&store);
                if let Err(e) = engine.reset(&EngineSyncAssociation::Disconnected) {
                    log::warn!("Failed to reset! {}", e);
                }
            }
            'W' | 'w' => {
                log::info!("Wiping all data from client!");
                if let Err(e) = store.wipe() {
                    log::warn!("Failed to wipe! {}", e);
                }
            }
            'S' | 's' => {
                log::info!("Syncing!");
                match store.sync(&cli_fxa.client_init, &cli_fxa.root_sync_key, &encryption_key) {
                    Err(e) => {
                        log::warn!("Sync failed! {}", e);
                        log::warn!("BT: {:?}", e.backtrace());
                    },
                    Ok(sync_ping) => {
                        log::info!("Sync was successful!");
                        log::info!("Sync telemetry: {}", serde_json::to_string_pretty(&sync_ping).unwrap());
                    }
                }
            }
            'V' | 'v' => {
                if let Err(e) = show_all(&store, &encdec) {
                    log::warn!("Failed to dump passwords? This is probably bad! {}", e);
                }
            }
            'B' | 'b' => {
                log::info!("Base Domain search");
                if let Some(basedomain) = prompt_string("Base domain (one line only, press enter when done):\n") {
                    match store.get_by_base_domain(&basedomain) {
                        Err(e) => {
                            log::warn!("Base domain lookup failed! {}", e);
                            log::warn!("BT: {:?}", e.backtrace());
                        },
                        Ok(result) => {
                            log::info!("Base domain result: {}", serde_json::to_string_pretty(&result).unwrap());
                        }
                    }
                }
            }
            'T' | 't' => {
                log::info!("Touching (bumping use count) for a record");
                match prompt_record_id(&store, &encdec, "update") {
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
                    if let Err(e) = show_sql(&store, &sql) {
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
