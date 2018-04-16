use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::sign::{Signer, Verifier};
use openssl::rsa::Rsa;
use serde_json;

use errors::*;
use super::{BrowserIDKeyPair, SigningPrivateKey, VerifyingPublicKey};

pub struct RSABrowserIDKeyPair {
  private_key: RSAPrivateKey,
  public_key: RSAPublicKey
}

impl BrowserIDKeyPair for RSABrowserIDKeyPair {
  fn private_key(&self) -> &SigningPrivateKey {
    return &self.private_key;
  }
  fn public_key(&self) -> &VerifyingPublicKey {
    return &self.public_key;
  }
}

struct RSAPrivateKey {
  key: PKey<Private>
}

impl SigningPrivateKey for RSAPrivateKey {
  fn get_algo(&self) -> String {
    "RS256".to_string()
  }

  fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
    let mut signer = Signer::new(MessageDigest::sha256(), &self.key)
      .chain_err(|| "Cannot instanciate the signer.")?;
    signer.update(message).chain_err(|| "Cannot feed data to signer.")?;
    signer.sign_to_vec().chain_err(|| "Cannot create signature.")
  }
}

struct RSAPublicKey {
  // These coeficients are base 10.
  n: String,
  e: String,
  key: PKey<Private>
}

impl VerifyingPublicKey for RSAPublicKey {
  fn to_json(&self) -> serde_json::Value {
    json!({
      "algorithm": "RS",
      "n": &self.n,
      "e": &self.e
    })
  }

  fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool> {
    let mut verifier = Verifier::new(MessageDigest::sha256(), &self.key)
      .chain_err(|| "Cannot instanciate the verifier.")?;
    verifier.update(message).chain_err(|| "Cannot feed data to verifier.")?;
    verifier.verify(signature).chain_err(|| "Cannot feed data to verifier.")
  }
}

pub fn generate_keypair(len: u32) -> Result<RSABrowserIDKeyPair> {
  let key_pair = Rsa::generate(len)
    .chain_err(|| "Could generate keypair.")?;
  let private_key = PKey::from_rsa(key_pair)
    .chain_err(|| "Could not get private key.")?;
  let rsa = private_key.rsa() // Awkward.
    .chain_err(|| "Could not get RSA struct.")?;
  let n = format!("{}", rsa.n().to_dec_str().chain_err(|| "Could not convert n.")?);
  let e = format!("{}", rsa.e().to_dec_str().chain_err(|| "Could not convert e.")?);
  let private_key_copy = PKey::from_rsa(rsa) // Sorry :(
    .chain_err(|| "Could not get private key.")?;
  Ok(RSABrowserIDKeyPair {
    private_key: RSAPrivateKey {key: private_key},
    public_key: RSAPublicKey {n, e, key: private_key_copy}
  })
}
