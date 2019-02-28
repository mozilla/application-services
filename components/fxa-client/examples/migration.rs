use fxa_client::FirefoxAccount;
use text_io::*;

static CONTENT_SERVER: &'static str = "https://fenixmigrator.dev.lcip.org";
static CLIENT_ID: &'static str = "3c49430b43dfba77";
static REDIRECT_URI: &'static str =
    "https://fenixmigrator.dev.lcip.org/oauth/success/3c49430b43dfba77";

fn main() {
    let mut fxa = FirefoxAccount::new(CONTENT_SERVER, CLIENT_ID, REDIRECT_URI);
    println!("Enter Session token (hex-string):");
    let session_token: String = read!("{}\n");
    println!("Enter kSync (hex-string):");
    let k_sync: String = read!("{}\n");
    println!("Enter kXCS (hex-string):");
    let k_xcs: String = read!("{}\n");
    fxa.migrate_from_session_token(&session_token, &k_sync, &k_xcs)
        .unwrap();
    println!("WOW! You've been migrated.");
    println!("JSON: {}", fxa.to_json().unwrap());
}
