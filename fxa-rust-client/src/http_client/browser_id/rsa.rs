use std;
use std::fmt;

use openssl::bn::BigNum;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private, Public};
use openssl::rsa::{Rsa, RsaPrivateKeyBuilder};
use openssl::sign::{Signer, Verifier};
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json;

use super::{BrowserIDKeyPair, SigningPrivateKey, VerifyingPublicKey};
use errors::*;

pub struct RSABrowserIDKeyPair {
    private_key: RSAPrivateKey,
    public_key: RSAPublicKey,
}

impl BrowserIDKeyPair for RSABrowserIDKeyPair {
    fn private_key(&self) -> &SigningPrivateKey {
        return &self.private_key;
    }
    fn public_key(&self) -> &VerifyingPublicKey {
        return &self.public_key;
    }
}

impl fmt::Debug for RSABrowserIDKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<rsa_key_pair>")
    }
}

impl Serialize for RSABrowserIDKeyPair {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("RSABrowserIDKeyPair", 2)?;
        state.serialize_field("n", &self.public_key.n)?;
        state.serialize_field("e", &self.public_key.e)?;
        state.serialize_field("d", &self.private_key.d)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for RSABrowserIDKeyPair {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            Modulus,
            PubExp,
            PrvExp,
        };

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`n`, `e` or `d`")
                    }

                    fn visit_str<E>(self, value: &str) -> std::result::Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "n" => Ok(Field::Modulus),
                            "e" => Ok(Field::PubExp),
                            "d" => Ok(Field::PrvExp),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct RSABrowserIDKeyPairVisitor;

        impl<'de> Visitor<'de> for RSABrowserIDKeyPairVisitor {
            type Value = RSABrowserIDKeyPair;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct RSABrowserIDKeyPair")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RSABrowserIDKeyPair, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut n = None;
                let mut e = None;
                let mut d = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Modulus => {
                            if n.is_some() {
                                return Err(de::Error::duplicate_field("n"));
                            }
                            n = Some(map.next_value()?);
                        }
                        Field::PubExp => {
                            if e.is_some() {
                                return Err(de::Error::duplicate_field("e"));
                            }
                            e = Some(map.next_value()?);
                        }
                        Field::PrvExp => {
                            if d.is_some() {
                                return Err(de::Error::duplicate_field("d"));
                            }
                            d = Some(map.next_value()?);
                        }
                    }
                }
                // TODO: Gross.
                let n = n.ok_or_else(|| de::Error::missing_field("n"))?;
                let n = BigNum::from_dec_str(n).unwrap();
                let n_copy = n.to_owned().unwrap();
                let e = e.ok_or_else(|| de::Error::missing_field("e"))?;
                let e = BigNum::from_dec_str(e).unwrap();
                let e_copy = e.to_owned().unwrap();
                let d = d.ok_or_else(|| de::Error::missing_field("d"))?;
                let d = BigNum::from_dec_str(d).unwrap();
                let public_key = create_public_key(n_copy, e_copy).unwrap();
                let private_key = create_private_key(n, e, d).unwrap();
                Ok(RSABrowserIDKeyPair {
                    private_key,
                    public_key,
                })
            }
        }

        const FIELDS: &'static [&'static str] = &["n", "e", "d"];
        deserializer.deserialize_struct("RSABrowserIDKeyPair", FIELDS, RSABrowserIDKeyPairVisitor)
    }
}

pub(crate) struct RSAPrivateKey {
    key: PKey<Private>,
    d: String,
}

impl SigningPrivateKey for RSAPrivateKey {
    fn get_algo(&self) -> String {
        format!("RS{}", self.key.bits() / 8)
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key)?;
        signer.update(message)?;
        Ok(signer.sign_to_vec()?)
    }
}

pub(crate) struct RSAPublicKey {
    key: PKey<Public>,
    n: String,
    e: String,
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
        let mut verifier = Verifier::new(MessageDigest::sha256(), &self.key)?;
        verifier.update(message)?;
        Ok(verifier.verify(signature)?)
    }
}

pub fn generate_keypair(len: u32) -> Result<RSABrowserIDKeyPair> {
    let rsa = Rsa::generate(len)?;
    let n = rsa.n().to_owned().unwrap();
    let e = rsa.e().to_owned().unwrap();
    let d = rsa.d().to_owned().unwrap();
    let d_str = format!("{}", d);
    let public_key = create_public_key(n, e)?;
    let private_key = RSAPrivateKey {
        key: PKey::from_rsa(rsa)?,
        d: d_str,
    };

    Ok(RSABrowserIDKeyPair {
        private_key,
        public_key,
    })
}

pub(crate) fn create_public_key(n: BigNum, e: BigNum) -> Result<RSAPublicKey> {
    let n_str = format!("{}", n.to_dec_str()?);
    let e_str = format!("{}", e.to_dec_str()?);
    let rsa = Rsa::from_public_components(n, e)?;
    let public_key = PKey::from_rsa(rsa)?;
    Ok(RSAPublicKey {
        key: public_key,
        n: n_str,
        e: e_str,
    })
}

pub(crate) fn create_private_key(n: BigNum, e: BigNum, d: BigNum) -> Result<RSAPrivateKey> {
    let d_str = format!("{}", d);
    let rsa = RsaPrivateKeyBuilder::new(n, e, d)?.build();
    let key = PKey::from_rsa(rsa)?;
    Ok(RSAPrivateKey { key, d: d_str })
}
