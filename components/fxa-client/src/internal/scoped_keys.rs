/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jwcrypto::{self, DecryptionParameters, Jwk};
use rc_crypto::{agreement, agreement::EphemeralKeyPair};

use super::FirefoxAccount;
use crate::{Error, Result, ScopedKey};

impl FirefoxAccount {
    pub(crate) fn get_scoped_key(&self, scope: &str) -> Result<&ScopedKey> {
        self.state
            .get_scoped_key(scope)
            .ok_or_else(|| Error::NoScopedKey(scope.to_string()))
    }
}

impl ScopedKey {
    pub fn key_bytes(&self) -> Result<Vec<u8>> {
        Ok(URL_SAFE_NO_PAD.decode(&self.k)?)
    }
}

impl std::fmt::Debug for ScopedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedKey")
            .field("kty", &self.kty)
            .field("scope", &self.scope)
            .field("kid", &self.kid)
            .finish()
    }
}

pub struct ScopedKeysFlow {
    key_pair: EphemeralKeyPair,
}

impl ScopedKeysFlow {
    pub fn with_random_key() -> Result<Self> {
        let key_pair = EphemeralKeyPair::generate(&agreement::ECDH_P256)?;
        Ok(Self { key_pair })
    }

    #[cfg(test)]
    pub fn from_static_key_pair(key_pair: agreement::KeyPair<agreement::Static>) -> Result<Self> {
        let (private_key, _) = key_pair.split();
        let ephemeral_prv_key = private_key._tests_only_dangerously_convert_to_ephemeral();
        let key_pair = agreement::KeyPair::from_private_key(ephemeral_prv_key)?;
        Ok(Self { key_pair })
    }

    pub fn get_public_key_jwk(&self) -> Result<Jwk> {
        Ok(jwcrypto::ec::extract_pub_key_jwk(&self.key_pair)?)
    }

    pub fn decrypt_keys_jwe(self, jwe: &str) -> Result<String> {
        let params = DecryptionParameters::ECDH_ES {
            local_key_pair: self.key_pair,
        };
        Ok(jwcrypto::decrypt_jwe(jwe, params)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jwcrypto::JwkKeyParameters;
    use rc_crypto::agreement::{KeyPair, PrivateKey};

    #[test]
    fn test_flow() {
        nss::ensure_initialized();
        let x = URL_SAFE_NO_PAD
            .decode("ARvGIPJ5eIFdp6YTM-INVDqwfun2R9FfCUvXbH7QCIU")
            .unwrap();
        let y = URL_SAFE_NO_PAD
            .decode("hk8gP0Po8nBh-WSiTsvsyesC5c1L6fGOEVuX8FHsvTs")
            .unwrap();
        let d = URL_SAFE_NO_PAD
            .decode("UayD4kn_4QHvLvLLSSaANfDUp9AcQndQu_TohQKoyn8")
            .unwrap();
        let ec_key =
            agreement::EcKey::from_coordinates(agreement::Curve::P256, &d, &x, &y).unwrap();
        let private_key = PrivateKey::<rc_crypto::agreement::Static>::import(&ec_key).unwrap();
        let key_pair = KeyPair::from(private_key).unwrap();
        let flow = ScopedKeysFlow::from_static_key_pair(key_pair).unwrap();
        let jwk = flow.get_public_key_jwk().unwrap();
        let ec_key_params = match jwk.key_parameters {
            JwkKeyParameters::EC(ref ec_key_params) => ec_key_params,
            _ => unreachable!("test only does EC"),
        };
        assert_eq!(ec_key_params.crv, "P-256");
        assert_eq!(
            ec_key_params.x,
            "ARvGIPJ5eIFdp6YTM-INVDqwfun2R9FfCUvXbH7QCIU"
        );
        assert_eq!(
            ec_key_params.y,
            "hk8gP0Po8nBh-WSiTsvsyesC5c1L6fGOEVuX8FHsvTs"
        );

        let jwe = "eyJhbGciOiJFQ0RILUVTIiwia2lkIjoiNFBKTTl5dGVGeUtsb21ILWd2UUtyWGZ0a0N3ak9HNHRfTmpYVXhLM1VqSSIsImVwayI6eyJrdHkiOiJFQyIsImNydiI6IlAtMjU2IiwieCI6IlB3eG9Na1RjSVZ2TFlKWU4wM2R0Y3o2TEJrR0FHaU1hZWlNQ3lTZXEzb2MiLCJ5IjoiLUYtTllRRDZwNUdSQ2ZoYm1hN3NvNkhxdExhVlNub012S0pFcjFBeWlaSSJ9LCJlbmMiOiJBMjU2R0NNIn0..b9FPhjjpmAmo_rP8.ur9jTry21Y2trvtcanSFmAtiRfF6s6qqyg6ruRal7PCwa7PxDzAuMN6DZW5BiK8UREOH08-FyRcIgdDOm5Zq8KwVAn56PGfcH30aNDGQNkA_mpfjx5Tj2z8kI6ryLWew4PGZb-PsL1g-_eyXhktq7dAhetjNYttKwSREWQFokv7N3nJGpukBqnwL1ost-MjDXlINZLVJKAiMHDcu-q7Epitwid2c2JVGOSCJjbZ4-zbxVmZ4o9xhFb2lbvdiaMygH6bPlrjEK99uT6XKtaIZmyDwftbD6G3x4On-CqA2TNL6ILRaJMtmyX--ctL0IrngUIHg_F0Wz94v.zBD8NACkUcZTPLH0tceGnA";
        let keys = flow.decrypt_keys_jwe(jwe).unwrap();
        assert_eq!(keys, "{\"https://identity.mozilla.com/apps/oldsync\":{\"kty\":\"oct\",\"scope\":\"https://identity.mozilla.com/apps/oldsync\",\"k\":\"8ek1VNk4sjrNP0DhGC4crzQtwmpoR64zHuFMHb4Tw-exR70Z2SSIfMSrJDTLEZid9lD05-hbA3n2Q4Esjlu1tA\",\"kid\":\"1526414944666-zgTjf5oXmPmBjxwXWFsDWg\"}}");
    }
}
