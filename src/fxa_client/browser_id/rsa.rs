use openssl::bn::BigNum;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Public, Private};
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

pub(crate) struct RSAPrivateKey {
  key: PKey<Private>
}

impl SigningPrivateKey for RSAPrivateKey {
  fn get_algo(&self) -> String {
    format!("RS{}", self.key.bits() / 8)
  }

  fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
    let mut signer = Signer::new(MessageDigest::sha256(), &self.key)
      .chain_err(|| "Cannot instanciate the signer.")?;
    signer.update(message).chain_err(|| "Cannot feed data to signer.")?;
    signer.sign_to_vec().chain_err(|| "Cannot create signature.")
  }
}

pub(crate) struct RSAPublicKey {
  key: PKey<Public>,
  n: String,
  e: String
}

impl VerifyingPublicKey for RSAPublicKey {
  fn to_json(&self) -> Result<serde_json::Value> {
    Ok(json!({
      "algorithm": "RS",
      "n": self.n,
      "e": self.e
    }))
  }

  fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool> {
    let mut verifier = Verifier::new(MessageDigest::sha256(), &self.key)
      .chain_err(|| "Cannot instanciate the verifier.")?;
    verifier.update(message).chain_err(|| "Cannot feed data to verifier.")?;
    verifier.verify(signature).chain_err(|| "Cannot feed data to verifier.")
  }
}

pub fn generate_keypair(len: u32) -> Result<RSABrowserIDKeyPair> {
  let rsa = Rsa::generate(len)
    .chain_err(|| "Could generate keypair.")?;
  let n = rsa.n().to_owned().unwrap();
  let e = rsa.e().to_owned().unwrap();
  let public_key = create_public_key(n, e)
    .chain_err(|| "Could not get public key.")?;
  let private_key = RSAPrivateKey {
    key: PKey::from_rsa(rsa).chain_err(|| "Could not get private key.")?
  };

  Ok(RSABrowserIDKeyPair {
    private_key,
    public_key
  })
}

pub(crate) fn create_public_key(n: BigNum, e: BigNum) -> Result<RSAPublicKey> {
  let n_str = format!("{}", n.to_dec_str().chain_err(|| "Could not convert n.")?);
  let e_str = format!("{}", e.to_dec_str().chain_err(|| "Could not convert e.")?);
  let rsa = Rsa::from_public_components(n, e)
    .chain_err(|| "Could not create rsa.")?;
  let public_key = PKey::from_rsa(rsa)
    .chain_err(|| "Could not get public key.")?;
  Ok(RSAPublicKey {
    key: public_key, n: n_str, e: e_str
  })
}

pub(crate) fn create_private_key(n: BigNum, e: BigNum, d: BigNum) -> Result<RSAPrivateKey> {
  let rsa = Rsa::build(n, e, d)
    .chain_err(|| "Could not create rsa.")?
    .build();
  let key = PKey::from_rsa(rsa).chain_err(|| "Could not get private key.")?;
  Ok(RSAPrivateKey {
    key
  })
}
