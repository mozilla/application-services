/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::BrowserIDKeyPair;
use crate::errors::*;
use openssl::{
    bn::BigNum,
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::{Rsa, RsaPrivateKeyBuilder},
    sign::{Signer, Verifier},
};
use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, Visitor},
    ser::{self, Serialize, SerializeStruct, Serializer},
};
use serde_json::{self, json};
use std::fmt;

pub struct RSABrowserIDKeyPair {
    key: PKey<Private>,
}

impl RSABrowserIDKeyPair {
    fn from_rsa(rsa: Rsa<Private>) -> Result<RSABrowserIDKeyPair> {
        let key = PKey::from_rsa(rsa)?;
        Ok(RSABrowserIDKeyPair { key })
    }

    pub fn generate_random(len: u32) -> Result<RSABrowserIDKeyPair> {
        let rsa = Rsa::generate(len)?;
        RSABrowserIDKeyPair::from_rsa(rsa)
    }

    #[allow(dead_code)]
    pub fn from_exponents_base10(n: &str, e: &str, d: &str) -> Result<RSABrowserIDKeyPair> {
        let n = BigNum::from_dec_str(n)?;
        let e = BigNum::from_dec_str(e)?;
        let d = BigNum::from_dec_str(d)?;
        let rsa = RsaPrivateKeyBuilder::new(n, e, d)?.build();
        RSABrowserIDKeyPair::from_rsa(rsa)
    }
}

impl BrowserIDKeyPair for RSABrowserIDKeyPair {
    fn get_algo(&self) -> String {
        format!("RS{}", self.key.bits() / 8)
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let mut signer = Signer::new(MessageDigest::sha256(), &self.key)?;
        signer.update(message)?;
        signer.sign_to_vec().map_err(Into::into)
    }

    fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool> {
        let mut verifier = Verifier::new(MessageDigest::sha256(), &self.key)?;
        verifier.update(message)?;
        verifier.verify(signature).map_err(Into::into)
    }

    fn to_json(&self, include_private: bool) -> Result<serde_json::Value> {
        if include_private {
            panic!("Not implemented!");
        }
        let rsa = self.key.rsa()?;
        let n = format!("{}", rsa.n().to_dec_str()?);
        let e = format!("{}", rsa.e().to_dec_str()?);
        Ok(json!({
            "algorithm": "RS",
            "n": n,
            "e": e
        }))
    }
}

impl Clone for RSABrowserIDKeyPair {
    fn clone(&self) -> RSABrowserIDKeyPair {
        let rsa = self.key.rsa().unwrap().clone();
        RSABrowserIDKeyPair::from_rsa(rsa).unwrap() // Yuck
    }
}

impl fmt::Debug for RSABrowserIDKeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<rsa_key_pair>")
    }
}

impl Serialize for RSABrowserIDKeyPair {
    #[allow(clippy::many_single_char_names)] // FIXME
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("RSABrowserIDKeyPair", 2)?;
        let rsa = self
            .key
            .rsa()
            .map_err(|err| ser::Error::custom(err.to_string()))?;
        let n = rsa
            .n()
            .to_dec_str()
            .map_err(|err| ser::Error::custom(err.to_string()))?;
        let e = rsa
            .e()
            .to_dec_str()
            .map_err(|err| ser::Error::custom(err.to_string()))?;
        let d = rsa
            .d()
            .to_dec_str()
            .map_err(|err| ser::Error::custom(err.to_string()))?;
        state.serialize_field("n", &format!("{}", n))?;
        state.serialize_field("e", &format!("{}", e))?;
        state.serialize_field("d", &format!("{}", d))?;
        if let (Some(p), Some(q)) = (rsa.p(), rsa.q()) {
            let p = p
                .to_dec_str()
                .map_err(|err| ser::Error::custom(err.to_string()))?;
            let q = q
                .to_dec_str()
                .map_err(|err| ser::Error::custom(err.to_string()))?;
            state.serialize_field("p", &format!("{}", p))?;
            state.serialize_field("q", &format!("{}", q))?;
        }
        if let (Some(dmp1), Some(dmq1), Some(iqmp)) = (rsa.dmp1(), rsa.dmq1(), rsa.iqmp()) {
            let dmp1 = dmp1
                .to_dec_str()
                .map_err(|err| ser::Error::custom(err.to_string()))?;
            let dmq1 = dmq1
                .to_dec_str()
                .map_err(|err| ser::Error::custom(err.to_string()))?;
            let iqmp = iqmp
                .to_dec_str()
                .map_err(|err| ser::Error::custom(err.to_string()))?;
            state.serialize_field("dmp1", &format!("{}", dmp1))?;
            state.serialize_field("dmq1", &format!("{}", dmq1))?;
            state.serialize_field("iqmp", &format!("{}", iqmp))?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for RSABrowserIDKeyPair {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            N,
            E,
            D,
            P,
            Q,
            Dmp1,
            Dmq1,
            Iqmp,
        };

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("`n`, `e`, `d`, `p`, `q`, `dmp1`, `dmq1`, `iqmp`")
                    }

                    fn visit_str<E>(self, value: &str) -> std::result::Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "n" => Ok(Field::N),
                            "e" => Ok(Field::E),
                            "d" => Ok(Field::D),
                            "p" => Ok(Field::P),
                            "q" => Ok(Field::Q),
                            "dmp1" => Ok(Field::Dmp1),
                            "dmq1" => Ok(Field::Dmq1),
                            "iqmp" => Ok(Field::Iqmp),
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

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("struct RSABrowserIDKeyPair")
            }
            #[allow(clippy::many_single_char_names)] // FIXME
            fn visit_map<V>(self, mut map: V) -> std::result::Result<RSABrowserIDKeyPair, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut n = None;
                let mut e = None;
                let mut d = None;
                let mut p = None;
                let mut q = None;
                let mut dmp1 = None;
                let mut dmq1 = None;
                let mut iqmp = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::N => {
                            if n.is_some() {
                                return Err(de::Error::duplicate_field("n"));
                            }
                            n = Some(map.next_value()?);
                        }
                        Field::E => {
                            if e.is_some() {
                                return Err(de::Error::duplicate_field("e"));
                            }
                            e = Some(map.next_value()?);
                        }
                        Field::D => {
                            if d.is_some() {
                                return Err(de::Error::duplicate_field("d"));
                            }
                            d = Some(map.next_value()?);
                        }
                        Field::P => {
                            if p.is_some() {
                                return Err(de::Error::duplicate_field("p"));
                            }
                            p = Some(map.next_value()?);
                        }
                        Field::Q => {
                            if q.is_some() {
                                return Err(de::Error::duplicate_field("q"));
                            }
                            q = Some(map.next_value()?);
                        }
                        Field::Dmp1 => {
                            if dmp1.is_some() {
                                return Err(de::Error::duplicate_field("dmp1"));
                            }
                            dmp1 = Some(map.next_value()?);
                        }
                        Field::Dmq1 => {
                            if dmq1.is_some() {
                                return Err(de::Error::duplicate_field("dmq1"));
                            }
                            dmq1 = Some(map.next_value()?);
                        }
                        Field::Iqmp => {
                            if iqmp.is_some() {
                                return Err(de::Error::duplicate_field("iqmp"));
                            }
                            iqmp = Some(map.next_value()?);
                        }
                    }
                }
                let n = n.ok_or_else(|| de::Error::missing_field("n"))?;
                let n =
                    BigNum::from_dec_str(n).map_err(|err| de::Error::custom(err.to_string()))?;
                let e = e.ok_or_else(|| de::Error::missing_field("e"))?;
                let e =
                    BigNum::from_dec_str(e).map_err(|err| de::Error::custom(err.to_string()))?;
                let d = d.ok_or_else(|| de::Error::missing_field("d"))?;
                let d =
                    BigNum::from_dec_str(d).map_err(|err| de::Error::custom(err.to_string()))?;
                let mut builder = RsaPrivateKeyBuilder::new(n, e, d)
                    .map_err(|err| de::Error::custom(err.to_string()))?;
                if let (Some(p), Some(q)) = (p, q) {
                    let p = BigNum::from_dec_str(p)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                    let q = BigNum::from_dec_str(q)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                    builder = builder
                        .set_factors(p, q)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                }
                if let (Some(dmp1), Some(dmq1), Some(iqmp)) = (dmp1, dmq1, iqmp) {
                    let dmp1 = BigNum::from_dec_str(dmp1)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                    let dmq1 = BigNum::from_dec_str(dmq1)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                    let iqmp = BigNum::from_dec_str(iqmp)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                    builder = builder
                        .set_crt_params(dmp1, dmq1, iqmp)
                        .map_err(|err| de::Error::custom(err.to_string()))?;
                }
                let rsa = builder.build();
                RSABrowserIDKeyPair::from_rsa(rsa).map_err(|err| de::Error::custom(err.to_string()))
            }
        }

        const FIELDS: &[&str] = &["n", "e", "d", "p", "q", "dmp1", "dmq1", "iqmp"];
        deserializer.deserialize_struct("RSABrowserIDKeyPair", FIELDS, RSABrowserIDKeyPairVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let key_pair = RSABrowserIDKeyPair::generate_random(2048).unwrap();
        let as_json = serde_json::to_string(&key_pair).unwrap();
        let _key_pair: RSABrowserIDKeyPair = serde_json::from_str(&as_json).unwrap();
    }
}
