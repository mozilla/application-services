
extern crate sync15_adapter;
extern crate error_chain;
extern crate url;
extern crate base64;
extern crate reqwest;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate env_logger;

use std::io::Read;
use std::error::Error;
use std::fs;
use std::process;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use sync15_adapter as sync;

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

fn start() -> Result<(), Box<Error>> {
    let oauth_data = do_auth(false)?;

    let scope = &oauth_data.keys["https://identity.mozilla.com/apps/oldsync"];

    let mut svc = sync::Sync15Service::new(
        sync::Sync15ServiceInit {
            key_id: scope.kid.clone(),
            sync_key: scope.k.clone(),
            access_token: oauth_data.access_token.clone(),
            tokenserver_base_url: "https://oauth-sync.dev.lcip.org/syncserver/token".into(),
        }
    )?;

    svc.remote_setup()?;
    let passwords = svc.all_records::<sync::record_types::PasswordRecord>("passwords")?;

    println!("Found {} passwords", passwords.len());

    for pw in passwords.iter() {
        println!("{:?}", pw.payload);
    }

    Ok(())
}

fn main() {
    env_logger::init();
    start().unwrap();
}
