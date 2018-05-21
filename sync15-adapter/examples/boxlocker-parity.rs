
extern crate sync15_adapter as sync;
extern crate error_chain;
extern crate url;
extern crate base64;
extern crate reqwest;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate env_logger;

use std::io::{self, Read, Write};
use std::error::Error;
use std::fs;
use std::process;
use std::time;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::rc::Rc;

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

fn do_auth(recur: bool) -> Result<OAuthCredentials, Box<Error>> {
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

fn read_login() -> sync::record_types::PasswordRecord {
    let username = prompt_string("username").unwrap_or(String::new());
    let password = prompt_string("password").unwrap_or(String::new());
    let form_submit_url = prompt_string("form_submit_url");
    let hostname = prompt_string("hostname");
    let http_realm = prompt_string("http_realm");
    let username_field = prompt_string("username_field").unwrap_or(String::new());
    let password_field = prompt_string("password_field").unwrap_or(String::new());
    let since_unix_epoch = time::SystemTime::now().duration_since(time::UNIX_EPOCH).unwrap();
    let dur_ms = since_unix_epoch.as_secs() * 1000 + ((since_unix_epoch.subsec_nanos() / 1_000_000) as u64);
    let ms_i64 = dur_ms as i64;
    sync::record_types::PasswordRecord {
        id: sync::util::random_guid().unwrap(),
        username,
        password,
        username_field,
        password_field,
        form_submit_url,
        http_realm,
        hostname,
        time_created: ms_i64,
        time_password_changed: ms_i64,
        times_used: None,
        time_last_used: Some(ms_i64),
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

fn start() -> Result<(), Box<Error>> {
    let oauth_data = do_auth(false)?;

    let scope = &oauth_data.keys["https://identity.mozilla.com/apps/oldsync"];

    let storage_client = Rc::new(sync::StorageClient::new(
        sync::StorageClientInit {
            key_id: scope.kid.clone(),
            access_token: oauth_data.access_token.clone(),
            tokenserver_base_url: "https://oauth-sync.dev.lcip.org/syncserver/token".into(),
        }
    )?);
    let mut collections = sync::Collections::new(storage_client.clone(), &scope.k)?;

    collections.remote_setup()?;
    let passwords = collections.all_records::<sync::record_types::PasswordRecord>("passwords")?
                       .into_iter()
                       .filter_map(|r| r.record())
                       .collect::<Vec<_>>();

    println!("Found {} passwords", passwords.len());

    for pw in passwords.iter() {
        println!("{:?}", pw.payload);
    }

    if !prompt_bool("Would you like to make changes? [y/N]").unwrap_or(false) {
        return Ok(());
    }

    let mut ids: Vec<String> = passwords.iter().map(|p| p.id.clone()).collect();

    let mut upd = sync::CollectionUpdate::new(&collections, false);
    loop {
        match prompt_chars("Add, delete, or commit [adc]:").unwrap_or('s') {
            'A' | 'a' => {
                let record = read_login();
                upd.add_record(record);
            },
            'D' | 'd' => {
                for (i, id) in ids.iter().enumerate() {
                    println!("{}: {}", i, id);
                }
                if let Some(index) = prompt_string("Index to delete (enter index)").and_then(|x| x.parse::<usize>().ok()) {
                    let result = ids.swap_remove(index);
                    upd.add_tombstone(result);
                } else {
                    println!("???");
                }
            },
            'C' | 'c' => {
                println!("committing!");
                let (good, bad) = upd.upload()?;
                println!("Uploded {} ids successfully, and {} unsuccessfully",
                         good.len(), bad.len());
                break;
            },
            c => {
                println!("Unknown action '{}', exiting.", c);
                break;
            }
        }
    }

    Ok(())
}

fn main() {
    env_logger::init();
    start().unwrap();
}
