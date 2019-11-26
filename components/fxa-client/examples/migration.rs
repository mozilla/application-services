use cli_support::prompt::prompt_string;
use fxa_client::FirefoxAccount;

static CLIENT_ID: &str = "3c49430b43dfba77";
//static CONTENT_SERVER: &str = "https://latest.dev.lcip.org";
static CONTENT_SERVER: &str = "http://127.0.0.1:3030";
//static REDIRECT_URI: &str = "https://latest.dev.lcip.org/oauth/success/3c49430b43dfba77";
static REDIRECT_URI: &str = "http://127.0.0.1:3030/oauth/success/3c49430b43dfba77";

fn main() {
    let mut fxa = FirefoxAccount::new(CONTENT_SERVER, CLIENT_ID, REDIRECT_URI);
    println!("Enter Session token (hex-string):");
    let session_token: String = prompt_string("session token").unwrap();
    println!("Enter kSync (hex-string):");
    let k_sync: String = prompt_string("k_sync").unwrap();
    println!("Enter kXCS (hex-string):");
    let k_xcs: String = prompt_string("k_xcs").unwrap();
    fxa.migrate_from_session_token(&session_token, &k_sync, &k_xcs, true)
        .unwrap();
    println!("WOW! You've been migrated.");
    println!("JSON: {}", fxa.to_json().unwrap());
}
