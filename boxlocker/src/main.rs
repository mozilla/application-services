
extern crate crypto;
extern crate reqwest;
extern crate hex;
extern crate base64;
extern crate hawk;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate hyper;
#[macro_use]
extern crate simple_error;
#[macro_use]
extern crate prettytable;

use std::io::prelude::*;
use std::fs::File;
use std::error::Error;
use std::collections::HashMap;
use std::fmt::Write;
use std::string::String;

//use crypto::mac::Mac;
use crypto::buffer::{ ReadBuffer, WriteBuffer };

const ISSUER: &str = "https://oauth-sync.dev.lcip.org";
const SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

header! { (XKeyID, "X-KeyID") => [String] }

fn main() {
  match syncit() {
    Ok(()) => {},
    Err(e) => println!("Error: {}", e.to_string()),
  }
}

fn syncit() -> Result<(), Box<Error>> {

    let client = reqwest::Client::new();

    // Automagically discover client config.

    let issuer_url = reqwest::Url::parse(ISSUER)?;
    let res = client.get(issuer_url.join("/.well-known/fxa-client-configuration")?)
        .send()?
        .text()?;
    let config: ClientConfig = serde_json::from_str(&res)?;

    // Slurp in credentials from disk.

    let mut f = File::open("./credentials.json")
        .expect("credentials.json not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents)?;
    let creds: OAuthCredentials = serde_json::from_str(&contents)?;

    let root_key = base64::decode_config(&creds.keys[SCOPE].k, base64::URL_SAFE_NO_PAD)?;
    let root_key_bundle = KeyBundle {
        enc_key: Vec::from(&root_key[..32]),
        mac_key: Vec::from(&root_key[32..64]),
    };

    println!("Loaded credentials from ./credentials.json");

    // Authenticate to tokenserver.

    let mut tokenserver_url = config.sync_tokenserver_base_url.clone();
    tokenserver_url.push_str("/1.0/sync/1.5");
    let tokenserver_url = reqwest::Url::parse(&tokenserver_url)?;

    let mut req = client.get(tokenserver_url)
        .build()?;

    let mut header = String::from("Bearer ");
    write!(&mut header, "{}", creds.access_token)?;
    req.headers_mut().set(reqwest::header::Authorization(header));
    req.headers_mut().set(XKeyID(creds.keys[SCOPE].kid.clone()));

    let res = client.execute(req)?
        .text()?;
    let sync_creds: SyncCredentials = serde_json::from_str(&res)?;

    // Fetch /crypto/keys from sync.

    let mut endpoint_url = sync_creds.api_endpoint.clone();
    endpoint_url.push_str("/");
    let endpoint_url = reqwest::Url::parse(&endpoint_url)?;

    let mut req = client.get(endpoint_url.join("storage/crypto/keys")?)
        .build()?;
    sync_creds.sign(&mut req)?;

    let res = client.execute(req)?
        .text()?;

    let keys: BSOEnvelope = serde_json::from_str(&res)?;

    // Unwrap the default keybundle.

    let keys = root_key_bundle.decrypt_bso(&keys)?;
    let keys: KeysBSO = serde_json::from_str(&keys)?;
    let sync_key_bundle = KeyBundle {
        enc_key: base64::decode(&keys.default[0])?,
        mac_key: base64::decode(&keys.default[1])?,
    };

    // Fetch /passwords from sync.

    let mut req = client.get(endpoint_url.join("storage/passwords?full=1")?)
        .build()?;
    sync_creds.sign(&mut req)?;

    let res = client.execute(req)?
        .text()?;

    let mut display_table = prettytable::Table::new();
    display_table.add_row(row!["id", "hostname", "username", "password"]);
    let items: serde_json::Value = serde_json::from_str(&res)?;
    match items {
        serde_json::Value::Array(items)=> {
            for item in items.iter() {
                // XXX TODO: is it necessary to round-trip via string?
                let password: BSOEnvelope = serde_json::from_str(serde_json::to_string(item)?.as_str())?;
                let password: PasswordBSO = serde_json::from_str(sync_key_bundle.decrypt_bso(&password)?.as_str())?;
                display_table.add_row(row![password.id, password.hostname, password.username, password.password]);
            }
        },
        _ => bail!("unexpected JSON type")
    };

    // That's it!

    display_table.printstd();

    return Ok(());
}


#[derive(Debug, Deserialize)]
struct ClientConfig {
    sync_tokenserver_base_url: String,
}

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
  access_token: String,
  refresh_token: String,
  keys: HashMap<String, ScopedKeyData>,
}

#[derive(Debug, Deserialize)]
struct SyncCredentials {
  api_endpoint: String,
  id: String,
  key: String,
}

#[derive(Debug, Deserialize)]
struct ScopedKeyData {
    k: String,
    kid: String,
    scope: String,
}


#[derive(Debug)]
struct KeyBundle {
    enc_key: Vec<u8>,
    mac_key: Vec<u8>,
}


#[derive(Debug, Deserialize)]
struct BSOEnvelope {
    id: String,
    payload: String,
    modified: f64,
}


#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct BSOPayload {
    ciphertext: String,
    IV: String,
    hmac: String,
}

#[derive(Debug, Deserialize)]
struct KeysBSO {
    default: Vec<String>,
}


#[derive(Debug, Deserialize)]
struct PasswordBSO {
    id: String,
    hostname: String,
    username: String,
    password: String,
}


impl SyncCredentials {

    fn sign(&self, req: &mut reqwest::Request) -> Result<(), Box<Error>> {
      let mut header = String::from("Hawk ");
      let credentials = hawk::Credentials{
        id: self.id.to_string(),
        key: hawk::Key::new(Vec::from(self.key.as_bytes()), &hawk::SHA256),
      };
      let mut path_qs = String::from(req.url().path());
      match req.url().query() {
        None => {},
        Some(qs) => {
          path_qs.push_str("?");
          path_qs.push_str(qs);
        }
      };
      write!(&mut header, "{}", hawk::RequestBuilder::new(
          req.method().as_ref(),
          match req.url().host_str() { Some(host) => host, None => bail!("missing host") },
          match req.url().port_or_known_default() { Some(port) => port, None => bail!("missing port") },
          &path_qs,
      ).request().make_header(&credentials)?)?;
      req.headers_mut().set(reqwest::header::Authorization(header));
      Ok(())
    }

}


impl KeyBundle {

    fn decrypt_bso(&self, bso: &BSOEnvelope) -> Result<String, Box<Error>> {
      let payload: BSOPayload = serde_json::from_str(&bso.payload)?;

      // XXX TODO: get this hmac-checking working...
      //
      //let expected_hmac = base64::decode(&payload.hmac)?;
      //let mut hmac = crypto::hmac::Hmac::new(crypto::sha2::Sha256::new(), &self.mac_key);
      //hmac.input(&payload.ciphertext.as_bytes());
      //if hmac.result() != crypto::mac::MacResult::new(&expected_hmac) {
      //  bail!("invalid hmac");
      //}
 
      let iv = base64::decode(&payload.IV)?;
      let ciphertext = base64::decode(&payload.ciphertext)?;

      // Decyryption code cargo-culted from
      // https://github.com/DaGenix/rust-crypto/blob/master/examples/symmetriccipher.rs

      let mut decryptor = crypto::aes::cbc_decryptor(
        crypto::aes::KeySize::KeySize256,
        &self.enc_key,
        &iv,
        crypto::blockmodes::PkcsPadding
      );
      
      let mut read_buffer = crypto::buffer::RefReadBuffer::new(&ciphertext);
      let mut buffer = [0u8; 4096];
      let mut write_buffer = crypto::buffer::RefWriteBuffer::new(&mut buffer);
      let mut plaintext: Vec<u8> = Vec::new();

      loop {
        let result = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true);
        plaintext.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));
        match result {
            Ok(crypto::buffer::BufferResult::BufferUnderflow) => break,
            Ok(crypto::buffer::BufferResult::BufferOverflow) => { },
            Err(err) => { println!("Crypto error: {:?}", err); bail!("crypto error") },
        }
      }

      Ok(String::from_utf8(plaintext)?)
    }

}
