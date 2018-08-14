// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

extern crate env_logger;
extern crate failure;
#[macro_use] extern crate prettytable;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate url;

extern crate logins;
extern crate mentat;
extern crate sync15_adapter as sync;
extern crate sync15_passwords;

use sync15_passwords::PasswordEngine;

use mentat::{
    DateTime,
    FromMillis,
    Utc,
};

use std::io::{self, Read, Write};
use failure::Error;
use std::fs;
use std::process;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
    access_token: String,
    refresh_token: String,
    keys: HashMap<String, ScopedKeyData>,
    expires_in: u64,
    auth_at: u64,
}

#[derive(Debug, Deserialize)]
struct ScopedKeyData {
    k: String,
    kid: String,
    scope: String,
}

use logins::{
    Credential,
    FormTarget,
    ServerPassword,
};
use logins::passwords;

fn do_auth(recur: bool) -> Result<OAuthCredentials, Error> {
    match fs::File::open("./credentials.json") {
        Err(_) => {
            if recur {
                panic!("Failed to open credentials 2nd time");
            }
            println!("No credentials found, invoking boxlocker.py...");
            process::Command::new("python")
                .arg("../boxlocker/boxlocker.py").output()
                .expect("Failed to run boxlocker.py");
            return do_auth(true);
        },
        Ok(mut file) => {
            let mut s = String::new();
            file.read_to_string(&mut s)?;
            let creds: OAuthCredentials = serde_json::from_str(&s)?;
            let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if creds.expires_in + creds.auth_at < time {
                println!("Warning, credentials may be stale.");
            }
            Ok(creds)
        }
    }
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

#[inline]
fn duration_ms(dur: Duration) -> u64 {
    dur.as_secs() * 1000 + ((dur.subsec_nanos() / 1_000_000) as u64)
}

#[inline]
fn unix_time_ms() -> u64 {
    duration_ms(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
}

fn read_login() -> ServerPassword {
    let username = prompt_string("username"); // .unwrap_or(String::new());
    let password = prompt_string("password").unwrap_or(String::new());
    let form_submit_url = prompt_string("form_submit_url");
    let hostname = prompt_string("hostname");
    let http_realm = prompt_string("http_realm");
    let username_field = prompt_string("username_field"); // .unwrap_or(String::new());
    let password_field = prompt_string("password_field"); // .unwrap_or(String::new());
    let ms_i64 = unix_time_ms() as i64;
    ServerPassword {
        uuid: sync::util::random_guid().unwrap().into(),
        username,
        password,
        username_field,
        password_field,
        target: match form_submit_url {
            Some(form_submit_url) => FormTarget::FormSubmitURL(form_submit_url),
            None => FormTarget::HttpRealm(http_realm.unwrap_or(String::new())), // XXX this makes little sense.
        },
        hostname: hostname.unwrap_or(String::new()), // XXX.
        time_created: DateTime::<Utc>::from_millis(ms_i64),
        time_password_changed: DateTime::<Utc>::from_millis(ms_i64),
        times_used: 0,
        time_last_used: DateTime::<Utc>::from_millis(ms_i64),

        modified: DateTime::<Utc>::from_millis(ms_i64), // XXX what should we do here?
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

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(|s| s.as_ref())
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

// fn update_login(record: &mut ServerPassword) {
//     update_string("username", &mut record.username, ", leave blank to keep");
//     let changed_password = update_string("password", &mut record.password, ", leave blank to keep");

//     if changed_password {
//         record.time_password_changed = unix_time_ms() as i64;
//     }

//     update_string("username_field", &mut record.username_field, ", leave blank to keep");
//     update_string("password_field", &mut record.password_field, ", leave blank to keep");

//     if prompt_bool(&format!("edit hostname? (now {}) [yN]", string_opt_or(&record.hostname, "(none)"))).unwrap_or(false) {
//         record.hostname = prompt_string("hostname");
//     }

//     if prompt_bool(&format!("edit form_submit_url? (now {}) [yN]", string_opt_or(&record.form_submit_url, "(none)"))).unwrap_or(false) {
//         record.form_submit_url = prompt_string("form_submit_url");
//     }
// }

fn update_credential(record: &mut Credential) {
    let mut username = record.username.clone().unwrap_or("".into());
    if update_string("username", &mut username, ", leave blank to keep") {
        record.username = Some(username);
    }
    update_string("password", &mut record.password, ", leave blank to keep");
    update_string("title",    &mut record.password, ", leave blank to keep");
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

fn as_table<'a, I>(records: I) -> (prettytable::Table, Vec<String>) where I: IntoIterator<Item=&'a ServerPassword> {
    let mut table = prettytable::Table::new();
    table.add_row(row![
            "(idx)", "id",
            "username", "password",
            "usernameField", "passwordField",
            "hostname",
            "formSubmitURL"
            // Skipping metadata so this isn't insanely long
        ]);

    let v: Vec<_> = records.into_iter().enumerate().map(|(index, rec)| {
        let target = match &rec.target {
            &FormTarget::FormSubmitURL(ref form_submit_url) => form_submit_url,
            &FormTarget::HttpRealm(ref http_realm) => http_realm,
        };

        table.add_row(row![
                index,
                rec.uuid.as_ref(),
                string_opt_or(&rec.username, "<username>"),
                &rec.password,
                string_opt_or(&rec.username_field, "<username_field>"),
                string_opt_or(&rec.password_field, "<password_field>"),
                &rec.hostname,
                target
            ]);

        rec.uuid.0.clone()
    }).collect();

    (table, v)
}

fn show_all(e: &mut PasswordEngine) -> Result<Vec<String>, Error> {
    let records = {
        let mut in_progress_read = e.store.begin_read()?;
            // .map_err(|_| "failed to begin_read")?;

        passwords::get_all_sync_passwords(&mut in_progress_read)?
            // .map_err(|_| "failed to get_all_sync_passwords")?
    };

    let (table, map) = as_table(&records);
    table.printstd();

    Ok(map)
}

fn prompt_record_id(e: &mut PasswordEngine, action: &str) -> Result<Option<String>, Error> {
    let index_to_id = show_all(e)?;
    let input = match prompt_usize(&format!("Enter (idx) of record to {}", action)) {
        Some(x) => x,
        None => {
            println!("Bad input");
            return Ok(None);
        },
    };

    if input >= index_to_id.len() {
        println!("No such index");
        return Ok(None);
    }

    Ok(Some(index_to_id[input].clone().into()))
}

fn main() -> Result<(), Error> {
    env_logger::init();
    let oauth_data = do_auth(false)?;

    let scope = &oauth_data.keys["https://identity.mozilla.com/apps/oldsync"];

    let client = sync::Sync15StorageClient::new(sync::Sync15StorageClientInit {
        key_id: scope.kid.clone(),
        access_token: oauth_data.access_token.clone(),
        tokenserver_url: url::Url::parse("https://oauth-sync.dev.lcip.org/syncserver/token/1.0/sync/1.5")?,
    })?;
    let mut sync_state = sync::GlobalState::default();

    let root_sync_key = sync::KeyBundle::from_ksync_base64(&scope.k)?;

    let mut state_machine =
        sync::SetupStateMachine::for_readonly_sync(&client, &root_sync_key);
    sync_state = state_machine.to_ready(sync_state)?;

    let mut engine = PasswordEngine::new(mentat::Store::open("logins.mentatdb")?)?;
    println!("Performing startup sync; engine has last server timestamp {}.", engine.last_server_timestamp);

    if let Err(e) = engine.sync(&client, &sync_state) {
        println!("Initial sync failed: {}", e);
        if !prompt_bool("Would you like to continue [yN]").unwrap_or(false) {
            return Err(e.into());
        }
    }

    show_all(&mut engine)?;

    loop {
        // match prompt_chars("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew, [R]eset, [W]ipe or [Q]uit").unwrap_or('?') {
        match prompt_chars("[T]ouch credential, [D]elete credential, [U]pdate credential, [S]ync, [V]iew, [R]eset, [W]ipe, or [Q]uit").unwrap_or('?') {
            'T' | 't' => {
                println!("Touching (recording usage of) credential");
                if let Some(id) = prompt_record_id(&mut engine, "touch (record usage of)")? {
                    // Here we're using that the credential uuid and the Sync 1.5 uuid are the same;
                    // that's not a stable assumption.
                    if let Err(e) = engine.touch_credential(id) {
                        println!("Failed to touch credential! {}", e);
                    }
                }
            }
            // 'A' | 'a' => {
            //     println!("Adding new record");
            //     let record = read_login();
            //     if let Err(e) = engine.create(record) {
            //         println!("Failed to create record! {}", e);
            //     }
            // }
            'D' | 'd' => {
                println!("Deleting credential");
                if let Some(id) = prompt_record_id(&mut engine, "delete")? {
                    // Here we're using that the credential uuid and the Sync 1.5 uuid are the same;
                    // that's not a stable assumption.
                    if let Err(e) = engine.delete_credential(id) {
                        println!("Failed to delete credential! {}", e);
                    }
                }
            }
            'U' | 'u' => {
                println!("Updating credential fields");
                if let Some(id) = prompt_record_id(&mut engine, "update")? {
                    // Here we're using that the credential uuid and the Sync 1.5 uuid are the same;
                    // that's not a stable assumption.
                    if let Err(e) = engine.update_credential(&id, update_credential) {
                        println!("Failed to update credential! {}", e);
                    }
                }
            }
            'R' | 'r' => {
                println!("Resetting client's last server timestamp (was {}).", engine.last_server_timestamp);
                if let Err(e) = engine.reset() {
                    println!("Failed to reset! {}", e);
                }
            }
            'W' | 'w' => {
                println!("Wiping all data from client!");
                if let Err(e) = engine.wipe() {
                    println!("Failed to wipe! {}", e);
                }
            }
            'S' | 's' => {
                println!("Syncing engine with last server timestamp {}!", engine.last_server_timestamp);
                if let Err(e) = engine.sync(&client, &sync_state) {
                    println!("Sync failed! {}", e);
                } else {
                    println!("Sync was successful!");
                }
            }
            'V' | 'v' => {
                // println!("Engine has {} records, a last sync timestamp of {}, and {} queued changes",
                //          engine.records.len(), engine.last_sync, engine.changes.len());
                println!("Engine has a last server timestamp of {}", engine.last_server_timestamp);

                { // Scope borrow of engine.
                    let in_progress_read = engine.store.begin_read()?;
                        // .map_err(|_| "failed to begin_read")?;

                    let deleted = passwords::get_deleted_sync_password_uuids_to_upload(&in_progress_read)?;
                        // .map_err(|_| "failed to get_deleted_sync_password_uuids_to_upload")?;
                    println!("{} deleted records to upload: {:?}", deleted.len(), deleted);

                    let modified = passwords::get_modified_sync_passwords_to_upload(&in_progress_read)?;
                        // .map_err(|_| "failed to get_modified_sync_passwords_to_upload")?;
                    println!("{} modified records to upload:", modified.len());

                    if !modified.is_empty() {
                        let (table, _map) = as_table(&modified);
                        table.printstd();
                    }
                }

                println!("Local collection:");
                show_all(&mut engine)?;
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
