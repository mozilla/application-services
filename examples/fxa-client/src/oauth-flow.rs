/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use cli_support::prompt::prompt_string;
use fxa_client::{FirefoxAccount, FxaConfig};
use std::collections::HashMap;
use url::Url;

const CLIENT_ID: &str = "7f368c6886429f19";
const REDIRECT_URI: &str = "https://mozilla.github.io/notes/fxa/android-redirect.html";
const SCOPES: &[&str] = &["https://identity.mozilla.com/apps/oldsync"];

fn main() {
    viaduct_reqwest::use_reqwest_backend();
    let config = FxaConfig::dev(CLIENT_ID, REDIRECT_URI);
    let fxa = FirefoxAccount::new(config);
    let url = fxa
        .begin_oauth_flow(SCOPES, "oauth_flow_example", None)
        .unwrap();
    println!("Open the following URL:");
    println!("{}", url);
    let redirect_uri: String = prompt_string("Obtained redirect URI").unwrap();
    let redirect_uri = Url::parse(&redirect_uri).unwrap();
    let query_params: HashMap<_, _> = redirect_uri.query_pairs().into_owned().collect();
    let code = &query_params["code"];
    let state = &query_params["state"];
    fxa.complete_oauth_flow(code, state).unwrap();
    let oauth_info = fxa
        .get_access_token(SCOPES[0], None)
        .expect("Error getting access token");
    println!("access_token: {:?}", oauth_info);
}
