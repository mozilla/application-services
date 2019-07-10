/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handle external Push Subscription Requests.
//!
//! "privileged" system calls may require additional handling and should be flagged as such.

use std::collections::HashMap;

use crate::communications::{connect, ConnectHttp, Connection, RegisterResponse};
use crate::config::PushConfiguration;
use crate::crypto::{Crypto, Cryptography, KeyV1 as Key};
use crate::storage::{Storage, Store};

use crate::error::{self, ErrorKind, Result};

pub struct PushManager {
    config: PushConfiguration,
    pub conn: ConnectHttp,
    pub store: Store,
}

impl PushManager {
    pub fn new(config: PushConfiguration) -> Result<Self> {
        let store = if let Some(ref path) = config.database_path {
            Store::open(path)?
        } else {
            Store::open_in_memory()?
        };
        let uaid = store.get_meta("uaid")?;
        let pm = PushManager {
            config: config.clone(),
            conn: connect(config, uaid.clone(), store.get_meta("auth")?)?,
            store,
        };
        Ok(pm)
    }

    // XXX: make these trait methods
    pub fn subscribe(&mut self, channel_id: &str, scope: &str) -> Result<(RegisterResponse, Key)> {
        let reg_token = self.config.registration_id.clone().unwrap();
        let subscription_key: Key;
        if let Some(uaid) = self.conn.uaid.clone() {
            // Don't fetch the connection from the server if we've already got one.
            if let Some(record) = self.store.get_record(&uaid, channel_id)? {
                return Ok((
                    RegisterResponse {
                        uaid,
                        channel_id: record.channel_id,
                        endpoint: record.endpoint,
                        secret: self.store.get_meta("auth")?,
                        senderid: Some(reg_token),
                    },
                    Key::deserialize(&record.key)?,
                ));
            }
        }
        let info = self.conn.subscribe(channel_id)?;
        if &self.config.sender_id == "test" {
            subscription_key = Crypto::test_key(
                "qJkxxWGVVxy7BKvraNY3hg8Gs-Y8qi0lRaXWJ3R3aJ8",
                "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50",
                "LsuUOBKVQRY6-l7_Ajo-Ag"
            )
        } else {
            subscription_key = Crypto::generate_key().unwrap();
        }
        // store the channel_id => auth + subscription_key
        let mut record = crate::storage::PushRecord::new(
            &info.uaid,
            &info.channel_id,
            &info.endpoint,
            scope,
            subscription_key.clone(),
        );
        record.app_server_key = self.config.vapid_key.clone();
        record.native_id = Some(reg_token);
        self.store.put_record(&record)?;
        // store the meta information if we've not yet done that.
        if self.store.get_meta("uaid")?.is_none() {
            self.store.set_meta("uaid", &info.uaid)?;
            if let Some(secret) = &info.secret {
                self.store.set_meta("auth", &secret)?;
            }
        }
        Ok((info, subscription_key))
    }

    // XXX: maybe -> Result<()> instead
    pub fn unsubscribe(&self, channel_id: Option<&str>) -> Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        let uaid = self.conn.uaid.as_ref().unwrap();
        Ok(if let Some(chid) = channel_id {
            self.conn.unsubscribe(channel_id)? && self.store.delete_record(uaid, chid)?
        } else {
            false
        })
    }

    pub fn unsubscribe_all(&self) -> Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        let uaid = self.conn.uaid.as_ref().unwrap();
        Ok({
            self.store.delete_all_records(uaid)?;
            self.conn.unsubscribe(None)?
        })
    }

    pub fn update(&mut self, new_token: &str) -> error::Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        let result = self.conn.update(&new_token)?;
        self.store
            .update_native_id(self.conn.uaid.as_ref().unwrap(), new_token)?;
        Ok(result)
    }

    pub fn verify_connection(&self) -> error::Result<bool> {
        if self.conn.uaid.is_none() {
            // Can't yet verify the channels, since no UAID has been set.
            // so return true for now.
            return Ok(true);
        }
        let channels = self
            .store
            .get_channel_list(self.conn.uaid.as_ref().unwrap())?;
        let result = self.conn.verify_connection(&channels)?;
        if !result {
            self.unsubscribe(None)?;
        }
        Ok(result)
    }

    pub fn decrypt(
        &self,
        uaid: &str,
        chid: &str,
        body: &str,
        encoding: &str,
        salt: Option<&str>,
        dh: Option<&str>,
    ) -> Result<String> {
        let val = self
            .store
            .get_record(&uaid, chid)
            .map_err(|e| ErrorKind::StorageError(format!("{:?}", e)))?
            .ok_or_else(|| {
                ErrorKind::StorageError(format!("No record for uaid:chid {:?}:{:?}", uaid, chid))
            })?;
        let key = Key::deserialize(&val.key)?;
        let decrypted = Crypto::decrypt(&key, body, encoding, salt, dh)
            .map_err(|e| ErrorKind::CryptoError(format!("{:?}", e)))?;
        serde_json::to_string(&decrypted)
            .map_err(|e| ErrorKind::TranscodingError(format!("{:?}", e)).into())
    }

    /// Fetch new endpoints for a list of channels.
    pub fn regenerate_endpoints(&mut self) -> error::Result<HashMap<String, String>> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions defined yet.".into()).into());
        }
        let uaid = self.conn.uaid.clone().unwrap();
        let channels = self.store.get_channel_list(&uaid)?;
        let mut results: HashMap<String, String> = HashMap::new();
        if &self.config.sender_id == "test" {
            results.insert(
                "deadbeef00000000decafbad00000000".to_owned(),
                "http://push.example.com/test/obscure".to_owned(),
            );
            return Ok(results);
        }
        for channel in channels {
            let info = self.conn.subscribe(&channel)?;
            self.store
                .update_endpoint(&uaid, &channel, &info.endpoint)?;
            results.insert(channel.clone(), info.endpoint);
        }
        Ok(results)
    }

    pub fn get_record_by_chid(
        &self,
        chid: &str,
    ) -> error::Result<Option<crate::storage::PushRecord>> {
        self.store.get_record_by_chid(chid)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic() -> Result<()> {
        let test_channel_id = "deadbeef00000000decafbad00000000";
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            ..Default::default()
        };
        let mut pm = PushManager::new(test_config)?;
        let (info, key) = pm.subscribe(test_channel_id, "")?;
        // verify that a subsequent request for the same channel ID returns the same subscription
        let (info2, key2) = pm.subscribe(test_channel_id, "")?;
        assert_eq!(
            Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned()),
            pm.store.get_meta("auth")?
        );
        assert_eq!(info.endpoint, info2.endpoint);
        assert_eq!(key, key2);
        assert!(pm.unsubscribe(Some(test_channel_id))?);
        // It's already deleted, so return false.
        assert!(!pm.unsubscribe(Some(test_channel_id))?);
        // No channel specified, so nothing done.
        assert!(!pm.unsubscribe(None)?);
        assert!(pm.unsubscribe_all()?);
        Ok(())
    }

    #[test]
    fn full() -> Result<()> {
        use rc_crypto::ece;
        use serde_json;

        let data_string = b"Mary had a little lamb, with some nice mint jelly";
        let test_channel_id = "deadbeef00000000decafbad00000000";
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            // database_path: Some("test.db"),
            ..Default::default()
        };
        let mut pm = PushManager::new(test_config)?;
        let (info, key) = pm.subscribe(test_channel_id, "")?;
        // Act like a subscription provider, so create a "local" key to encrypt the data
        let mut auth_secret = vec![0u8; 16];
        let mut salt = vec![0u8; 16];
        rc_crypto::rand::fill(&mut auth_secret).unwrap();
        rc_crypto::rand::fill(&mut salt).unwrap();
        let ciphertext = ece::encrypt(&key.public_key(), &key.auth, &salt, data_string).unwrap();
        let body = base64::encode_config(&ciphertext, base64::URL_SAFE_NO_PAD);

        let result = pm
            .decrypt(&info.uaid, &info.channel_id, &body, "aes128gcm", None, None)
            .unwrap();
        assert_eq!(
            serde_json::to_string(&data_string.to_vec()).unwrap(),
            result
        );
        Ok(())
    }

}
