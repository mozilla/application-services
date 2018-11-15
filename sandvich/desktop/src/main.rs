extern crate fxa_client;
#[macro_use] extern crate text_io;
extern crate url;

use fxa_client::{Config, FirefoxAccount};
use std::collections::HashMap;
use url::Url;

static CONTENT_SERVER: &'static str = "http://127.0.0.1:3030";
static CLIENT_ID: &'static str = "7f368c6886429f19";
static REDIRECT_URI: &'static str = "https://mozilla.github.io/notes/fxa/android-redirect.html";
static SCOPES: &'static [&'static str] = &["https://identity.mozilla.com/apps/oldsync"];

fn main() {
    let config = Config::import_from(CONTENT_SERVER, CLIENT_ID, REDIRECT_URI).unwrap();
    let mut fxa = FirefoxAccount::new(config);
    let url = fxa.begin_oauth_flow(&SCOPES, false).unwrap();
    println!("Open the following URL:");
    println!("{}", url);
    println!("Obtained redirect URI:");
    let redirect_uri: String = read!("{}\n");
    let redirect_uri = Url::parse(&redirect_uri).unwrap();
    let query_params: HashMap<_, _> = redirect_uri.query_pairs().into_owned().collect();
    let code = query_params.get("code").unwrap();
    let state = query_params.get("state").unwrap();
    let oauth_info = fxa.complete_oauth_flow(&code, &state).unwrap();
    println!("access_token: {:?}", oauth_info);
}
