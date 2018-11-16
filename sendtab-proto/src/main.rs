extern crate base64;
extern crate fxa_client;
extern crate dialoguer;
extern crate ece;
#[macro_use] extern crate text_io;
extern crate url;
extern crate serde;
#[macro_use]
extern crate serde_json;
extern crate sync15_adapter as sync;

use dialoguer::Select;
use ece::{Aes128GcmEceWebPush, OpenSSLRemotePublicKey, OpenSSLLocalKeyPair, WebPushParams};
use fxa_client::{Config, FirefoxAccount};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sync::KeyBundle;
use url::Url;

static CONTENT_SERVER: &'static str = "http://127.0.0.1:3030";
static CLIENT_ID: &'static str = "a48174070bc7322d";
static REDIRECT_URI: &'static str = "https://lockbox.firefox.com/fxa/android-redirect.html";
static COMMAND_SENDTAB: &'static str = "https://identity.mozilla.com/cmd/open-uri";
static SCOPES: &'static [&'static str] = &["https://identity.mozilla.com/apps/oldsync", "clients:read", "commands:write"];
static INSTANCE_NAME: &'static str = "Baby's first OAuth client instance";

static PUSH_ENDPOINT: &'static str = "https://updates.push.services.mozilla.com/wpush/v1/gAAAAABb6KbBWpZFYjDyeQEN5NJ9XCBxl3VYjwxURWL2yT-RNB51f-KxPIrTphLm52hc0fvEMra7PoNxMppnajNpktBUYmA6OYc_jK87Ng1LPeeOGqqg4ipdh52JdBhOkb9VPxLy7H_n";
static PRV_KEY: &'static str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
static PUB_KEY: &'static str = "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
static AUTH_SECRET: &'static str = "BTBZMqHH6r4Tts7J_aSIgg";

fn main() {
    let config = Config::import_from(CONTENT_SERVER).unwrap();
    let mut fxa = FirefoxAccount::new(config, CLIENT_ID, REDIRECT_URI);

    // OAuth access token
    println!("Let's get an access token first");
    let url = fxa.begin_oauth_flow(&SCOPES, true).unwrap();
    println!("Open the following URL in a browser:");
    println!("{}", url);
    println!("Obtained redirect URI:");
    let redirect_uri: String = read!("{}\n");
    let redirect_uri = Url::parse(&redirect_uri).unwrap();
    let query_params: HashMap<_, _> = redirect_uri.query_pairs().into_owned().collect();
    let code = query_params.get("code").unwrap();
    let state = query_params.get("state").unwrap();
    fxa.complete_oauth_flow(&code, &state).unwrap();
    println!("Login OK");

    // Set a default client instance name.
    fxa.set_name(INSTANCE_NAME).unwrap();

    // TODO: background tab receiving.
    // let fxa: Arc<Mutex<FirefoxAccount>> = Arc::new(Mutex::new(fxa));
    // {
    //   let fxa = fxa.clone();
    //   thread::spawn(move || {
    //     loop {
    //       let auth_key = base64::decode_config(AUTH_SECRET, base64::URL_SAFE_NO_PAD).unwrap();
    //       let prv_key = base64::decode_config(PRV_KEY, base64::URL_SAFE_NO_PAD).unwrap();
    //       let prv_key = OpenSSLLocalKeyPair::new(&prv_key).unwrap();
    //       let pending_commands = match fxa.lock().unwrap().pending_commands() {
    //         Ok(res) => res,
    //         Err(_) => {
    //           // Heh it's probably a 404
    //           let three_secs = time::Duration::from_secs(3);
    //           thread::sleep(three_secs);
    //           continue
    //         }
    //       };
    //       for msg in pending_commands.messages {
    //         let encrypted = msg.data.payload["encrypted"].as_str().unwrap();
    //         let encrypted = base64::decode_config(encrypted, base64::URL_SAFE_NO_PAD).unwrap();
    //         let decrypted = Aes128GcmEceWebPush::decrypt(&prv_key, &auth_key, &encrypted).unwrap();
    //         let data = String::from_utf8(decrypted).unwrap();
    //         let data: serde_json::Value = serde_json::from_str(&data).unwrap();
    //         let tab_url = &data["entries"][0]["url"];
    //         println!("Received: {}", tab_url);
    //       }
    //       let three_secs = time::Duration::from_secs(3);
    //       thread::sleep(three_secs);
    //     }
    //   });
    // }

    // Menu:
    loop {
      println!("Main menu:");
      let mut main_menu = Select::new();
      main_menu.items(&["Set Name", "Send a Tab", "Quit"]);
      main_menu.default(0);
      let main_menu_selection = main_menu.interact().unwrap();

      match main_menu_selection {
        0 => {
          println!("Enter new name:");
          let new_name: String = read!("{}\n");
          // Set client instance name
          fxa.set_name(&new_name).unwrap();
          println!("Client Instance name set to: {}", new_name);
        },
        1 => {
          let instances = fxa.clients_instances().unwrap();
          let instances_names: Vec<String> = instances.iter().map(|i| i.name.clone().unwrap_or(i.id.clone())).collect();
          let mut targets_menu = Select::new();
          targets_menu.default(0);
          let instances_names_refs: Vec<&str> = instances_names.iter().map(|s| s.as_ref()).collect();
          targets_menu.items(&instances_names_refs);
          println!("Choose a send-tab target:");
          let selection = targets_menu.interact().unwrap();
          let target = instances.get(selection).unwrap();

          // Payload
          println!("URL:");
          let url: String = read!("{}\n");
          let data = json!({
            "entries": [{
              "title": "(Empty title)",
              "url": url,
            }]
          });
          let data_str = serde_json::to_string(&data).unwrap();
          let bytes = data_str.as_bytes();

          // Push Keys
          let bundle = match target.available_commands {
            Some(ref commands) => commands.get(COMMAND_SENDTAB).unwrap(),
            None => panic!("No cmd")
          };
          let bundle: serde_json::Value = serde_json::from_str(bundle).unwrap();
          let iv = bundle["IV"].as_str().unwrap();
          let iv = base64::decode(&iv).unwrap();
          let cipher_keys = bundle["ciphertext"].as_str().unwrap();
          let cipher_keys = base64::decode(&cipher_keys).unwrap();
          let oldsync_token = fxa.get_access_token("https://identity.mozilla.com/apps/oldsync").unwrap();
          let k_sync_scopped: serde_json::Value = serde_json::from_str(&oldsync_token.key.unwrap()).unwrap();
          let k_sync = k_sync_scopped["k"].as_str().unwrap();
          let root_sync_key = KeyBundle::from_ksync_base64(&k_sync).unwrap();
          let keys_json = root_sync_key.decrypt(&cipher_keys, &iv).unwrap();
          let keys_json: serde_json::Value = serde_json::from_str(&keys_json).unwrap();
          let public_key = keys_json["publicKey"].as_str().unwrap();
          let auth_secret = keys_json["authSecret"].as_str().unwrap();
          let public_key = base64::decode_config(&public_key, base64::URL_SAFE_NO_PAD).unwrap();
          let public_key = OpenSSLRemotePublicKey::from_raw(&public_key);
          let auth_secret = base64::decode_config(&auth_secret, base64::URL_SAFE_NO_PAD).unwrap();

          // Encryption
          let encrypted = Aes128GcmEceWebPush::encrypt(&public_key, &auth_secret, bytes, WebPushParams::default()).unwrap();
          let encrypted = base64::encode_config(&encrypted, base64::URL_SAFE_NO_PAD);

          // Request
          let payload = json!({
            "encrypted": encrypted
          });
          fxa.invoke_command(COMMAND_SENDTAB, &target.id, &payload).unwrap();
          println!("Tab sent!");
        },
        2 => ::std::process::exit(0),
        _ => {panic!("Invalid choice!")}
      }
    }
}
