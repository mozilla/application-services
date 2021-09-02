/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handle external Push Subscription Requests.
//!
//! "privileged" system calls may require additional handling and should be flagged as such.

use crate::communications::{
    connect, ConnectHttp, Connection, PersistedRateLimiter, RegisterResponse,
};
use crate::config::PushConfiguration;
use crate::crypto::{Crypto, Cryptography, KeyV1 as Key};
use crate::error::{self, ErrorKind, Result};
use crate::storage::{PushRecord, Storage, Store};

const UPDATE_RATE_LIMITER_INTERVAL: u64 = 24 * 60 * 60; // 500 calls per 24 hours.
const UPDATE_RATE_LIMITER_MAX_CALLS: u16 = 500;

pub struct PushManager {
    config: PushConfiguration,
    pub conn: ConnectHttp,
    pub store: Store,
    update_rate_limiter: PersistedRateLimiter,
}

impl PushManager {
    pub fn new(config: PushConfiguration) -> Result<Self> {
        let store = if let Some(ref path) = config.database_path {
            Store::open(path)?
        } else {
            Store::open_in_memory()?
        };
        let uaid = store.get_meta("uaid")?;
        Ok(Self {
            config: config.clone(),
            conn: connect(config, uaid, store.get_meta("auth")?)?,
            store,
            update_rate_limiter: PersistedRateLimiter::new(
                "update_token",
                UPDATE_RATE_LIMITER_INTERVAL,
                UPDATE_RATE_LIMITER_MAX_CALLS,
            ),
        })
    }

    // XXX: make these trait methods
    pub fn subscribe(
        &mut self,
        channel_id: &str,
        scope: &str,
        server_key: Option<&str>,
    ) -> Result<(RegisterResponse, Key)> {
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
        let info = self.conn.subscribe(channel_id, server_key)?;
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
        record.app_server_key = server_key.map(|v| v.to_owned());
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

    pub fn unsubscribe(&mut self, channel_id: &str) -> Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        self.conn.unsubscribe(channel_id)?;
        self.store
            .delete_record(self.conn.uaid.as_ref().unwrap(), channel_id)
    }

    pub fn unsubscribe_all(&mut self) -> Result<()> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        let uaid = self.conn.uaid.as_ref().unwrap();
        self.store.delete_all_records(uaid)?;
        self.conn.unsubscribe_all()?;
        Ok(())
    }

    pub fn update(&mut self, new_token: &str) -> error::Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        if !self.update_rate_limiter.check(&self.store) {
            return Ok(false);
        }
        self.conn.update(&new_token)?;
        self.store
            .update_native_id(self.conn.uaid.as_ref().unwrap(), new_token)?;
        Ok(true)
    }

    pub fn verify_connection(&mut self) -> Result<Vec<PushRecord>> {
        let uaid = self
            .conn
            .uaid
            .as_ref()
            .ok_or_else(|| ErrorKind::GeneralError("No subscriptions created yet.".into()))?
            .to_owned();

        let channels = self.store.get_channel_list(&uaid)?;
        if self.conn.verify_connection(&channels)? {
            // Everything is fine, our subscriptions in the db match the remote server.
            return Ok(Vec::new());
        }

        let mut subscriptions: Vec<PushRecord> = Vec::new();
        for channel in channels {
            if let Some(record) = self.store.get_record_by_chid(&channel)? {
                subscriptions.push(record);
            }
        }
        // we wipe the UAID if there is a mismatch, forcing us to later
        // re-generate a new one when we do the next first subscription.
        // this is to prevent us from attempting to communicate with the server using an outdated
        // UAID, the in-memory uaid was already wiped in the `verify_connection` call
        // when we unsubscribe
        self.store.delete_all_records(&uaid)?;
        Ok(subscriptions)
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
            .ok_or_else(|| ErrorKind::RecordNotFoundError(uaid.to_owned(), chid.to_owned()))?;
        let key = Key::deserialize(&val.key)?;
        let decrypted = Crypto::decrypt(&key, body, encoding, salt, dh)
            .map_err(|e| ErrorKind::CryptoError(format!("{:?}", e)))?;
        serde_json::to_string(&decrypted)
            .map_err(|e| ErrorKind::TranscodingError(format!("{:?}", e)).into())
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
    const TEST_CHANNEL_ID: &str = "deadbeef00000000decafbad00000000";
    #[test]
    fn basic() -> Result<()> {
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            ..Default::default()
        };
        let mut pm = PushManager::new(test_config)?;
        let (info, key) = pm.subscribe(TEST_CHANNEL_ID, "", None)?;
        // verify that a subsequent request for the same channel ID returns the same subscription
        let (info2, key2) = pm.subscribe(TEST_CHANNEL_ID, "", None)?;
        assert_eq!(
            Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned()),
            pm.store.get_meta("auth")?
        );
        assert_eq!(info.endpoint, info2.endpoint);
        assert_eq!(key, key2);
        assert!(pm.unsubscribe(TEST_CHANNEL_ID)?);
        // It's already deleted, so return false.
        assert!(!pm.unsubscribe(TEST_CHANNEL_ID)?);
        pm.unsubscribe_all()?;
        Ok(())
    }

    #[test]
    fn full() -> Result<()> {
        use rc_crypto::ece;
        rc_crypto::ensure_initialized();
        let data_string = b"Mary had a little lamb, with some nice mint jelly";
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            // database_path: Some("test.db"),
            ..Default::default()
        };
        let mut pm = PushManager::new(test_config)?;
        let (info, key) = pm.subscribe(TEST_CHANNEL_ID, "", None)?;
        // Act like a subscription provider, so create a "local" key to encrypt the data
        let ciphertext = ece::encrypt(&key.public_key(), &key.auth, data_string).unwrap();
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

    #[test]
    fn test_wipe_uaid() -> Result<()> {
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            ..Default::default()
        };
        let mut pm = PushManager::new(test_config)?;
        let (info, _) = pm.subscribe(TEST_CHANNEL_ID, "", None)?;
        // verify that the uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(pm.store.get_meta("uaid")?.unwrap(), info.uaid);
        assert_eq!(
            pm.store
                .get_record(&info.uaid, TEST_CHANNEL_ID)?
                .unwrap()
                .channel_id,
            TEST_CHANNEL_ID
        );
        let unsubscribed_channels = pm.verify_connection()?;
        assert_eq!(unsubscribed_channels.len(), 1);
        assert_eq!(unsubscribed_channels[0].channel_id, TEST_CHANNEL_ID);
        // since verify_connection failed,
        // we wipe the uaid and all associated records from our store
        assert!(pm.store.get_meta("uaid")?.is_none());
        assert!(pm.store.get_record(&info.uaid, TEST_CHANNEL_ID)?.is_none());

        // we now check that a new subscription will cause us to
        // re-generate a uaid and store it in our store
        let (info, _) = pm.subscribe(TEST_CHANNEL_ID, "", None)?;
        // verify that the uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(pm.store.get_meta("uaid")?.unwrap(), info.uaid);
        assert_eq!(
            pm.store
                .get_record(&info.uaid, TEST_CHANNEL_ID)?
                .unwrap()
                .channel_id,
            TEST_CHANNEL_ID
        );
        Ok(())
    }
}
