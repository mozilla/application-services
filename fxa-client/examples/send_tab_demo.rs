/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate dialoguer;
extern crate failure;
extern crate fxa_client;
#[macro_use]
extern crate text_io;
extern crate url;
extern crate webbrowser;

use dialoguer::Select;
use fxa_client::{Config, FirefoxAccount, PersistCallback, TabReceivedCallback};
use std::{collections::HashMap, fs, io::{Read, Write}, sync::{Arc, Mutex}, time, thread};
use url::Url;

static CREDENTIALS_PATH: &'static str = "credentials.json";
static CONTENT_SERVER: &'static str = "http://127.0.0.1:3030";
static CLIENT_ID: &'static str = "a48174070bc7322d";
static REDIRECT_URI: &'static str = "https://lockbox.firefox.com/fxa/android-redirect.html";
static SCOPES: &'static [&'static str] = &[
    "https://identity.mozilla.com/apps/oldsync",
    "clients:read",
    "commands:write",
];
static INSTANCE_NAME: &'static str = "Baby's first instance";

fn load_fxa_creds() -> Result<FirefoxAccount, failure::Error> {
    let mut file = fs::File::open(CREDENTIALS_PATH)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn load_or_create_fxa_creds(cfg: Config) -> Result<FirefoxAccount, failure::Error> {
    let mut acct = load_fxa_creds()
    .or_else(|_e| {
        create_fxa_creds(cfg)
    })?;
    acct.register_persist_callback(PersistCallback::new(persist_fxa_state));
    Ok(acct)
}

fn persist_fxa_state(json: &str) {
    let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(true)
            .create(true)
            .open(CREDENTIALS_PATH)
            .unwrap();
    write!(file, "{}", json).unwrap();
    file.flush().unwrap();
}

fn create_fxa_creds(cfg: Config) -> Result<FirefoxAccount, failure::Error> {
    let mut acct = FirefoxAccount::with_config(cfg);
    let oauth_uri = acct.begin_oauth_flow(&SCOPES, true)?;

    if let Err(_) = webbrowser::open(&oauth_uri.as_ref()) {
        println!("Please visit this URL, sign in, and then copy-paste the final URL below.");
        println!("\n    {}\n", oauth_uri);
    } else {
        println!("Please paste the final URL below:\n");
    }

    let redirect_uri: String = read!("{}\n");
    let redirect_uri = Url::parse(&redirect_uri).unwrap();
    let query_params: HashMap<_, _> = redirect_uri.query_pairs().into_owned().collect();
    let code = query_params.get("code").unwrap();
    let state = query_params.get("state").unwrap();
    acct.complete_oauth_flow(&code, &state).unwrap();
    persist_fxa_state(&acct.to_json().unwrap());
    // Set a default display name.
    acct.set_display_name(INSTANCE_NAME).unwrap();
    Ok(acct)
}

fn on_tab_received(_title: &str, url: &str) {
    webbrowser::open(&url).unwrap();
}

fn main() -> Result<(), failure::Error> {
    let cfg = Config::new(CONTENT_SERVER, CLIENT_ID, REDIRECT_URI);
    let mut acct = load_or_create_fxa_creds(cfg.clone())?;

    // Initialize the send tab command handler and register our callback.
    acct.init_send_tab(TabReceivedCallback::new(on_tab_received)).unwrap();

    let acct: Arc<Mutex<FirefoxAccount>> = Arc::new(Mutex::new(acct));
    {
      let acct = acct.clone();
      thread::spawn(move || {
        loop {
            acct.lock().unwrap().fetch_missed_remote_commands().is_ok(); // Ignore 404 errors for now.
            thread::sleep(time::Duration::from_secs(1));
        }
      });
    }

    // Menu:
    loop {
        println!("Main menu:");
        let mut main_menu = Select::new();
        main_menu.items(&["Set Display Name", "Send a Tab", "Quit"]);
        main_menu.default(0);
        let main_menu_selection = main_menu.interact().unwrap();

        match main_menu_selection {
            0 => {
                println!("Enter new display name:");
                let new_name: String = read!("{}\n");
                // Set instance display name
                acct.lock().unwrap().set_display_name(&new_name).unwrap();
                println!("Display name set to: {}", new_name);
            }
            1 => {
                let instances = acct.lock().unwrap().get_instances().unwrap();
                let instances_names: Vec<String> = instances
                    .iter()
                    .map(|i| i.name.clone().unwrap_or(i.id.clone()))
                    .collect();
                let mut targets_menu = Select::new();
                targets_menu.default(0);
                let instances_names_refs: Vec<&str> =
                    instances_names.iter().map(|s| s.as_ref()).collect();
                targets_menu.items(&instances_names_refs);
                println!("Choose a send-tab target:");
                let selection = targets_menu.interact().unwrap();
                let target = instances.get(selection).unwrap();

                // Payload
                println!("Title:");
                let title: String = read!("{}\n");
                println!("URL:");
                let url: String = read!("{}\n");
                acct.lock().unwrap().send_tab(target, &title, &url).unwrap();
                println!("Tab sent!");
            }
            2 => ::std::process::exit(0),
            _ => panic!("Invalid choice!"),
        }
    }
}
