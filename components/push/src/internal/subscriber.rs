/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handle external Push Subscription Requests.
//!
//! "privileged" system calls may require additional handling and should be flagged as such.

use crate::error::{self, PushError, Result};
use crate::internal::communications::{
    connect, ConnectHttp, Connection, PersistedRateLimiter, RegisterResponse,
};
use crate::internal::config::PushConfiguration;
use crate::internal::crypto::{Crypto, Cryptography, KeyV1 as Key};
use crate::internal::storage::{PushRecord, Storage, Store};
use crate::{
    DispatchInfo, KeyInfo, PushSubscriptionChanged, SubscriptionInfo, SubscriptionResponse,
};

const UPDATE_RATE_LIMITER_INTERVAL: u64 = 24 * 60 * 60; // 500 calls per 24 hours.
const UPDATE_RATE_LIMITER_MAX_CALLS: u16 = 500;

impl From<(RegisterResponse, Key)> for SubscriptionResponse {
    fn from(val: (RegisterResponse, Key)) -> Self {
        SubscriptionResponse {
            channel_id: val.0.channel_id,
            subscription_info: SubscriptionInfo {
                endpoint: val.0.endpoint,
                keys: val.1.into(),
            },
        }
    }
}

impl From<Key> for KeyInfo {
    fn from(key: Key) -> Self {
        KeyInfo {
            auth: base64::encode_config(&key.auth, base64::URL_SAFE_NO_PAD),
            p256dh: base64::encode_config(&key.public_key(), base64::URL_SAFE_NO_PAD),
        }
    }
}

impl From<PushRecord> for PushSubscriptionChanged {
    fn from(record: PushRecord) -> Self {
        PushSubscriptionChanged {
            channel_id: record.channel_id,
            scope: record.scope,
        }
    }
}

impl From<PushRecord> for DispatchInfo {
    fn from(record: PushRecord) -> Self {
        DispatchInfo {
            scope: record.scope,
            endpoint: record.endpoint,
            app_server_key: record.app_server_key,
        }
    }
}

pub struct PushManager {
    config: PushConfiguration,
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
        Ok(Self {
            config,
            store,
            update_rate_limiter: PersistedRateLimiter::new(
                "update_token",
                UPDATE_RATE_LIMITER_INTERVAL,
                UPDATE_RATE_LIMITER_MAX_CALLS,
            ),
        })
    }

    pub fn make_connection(&self) -> Result<ConnectHttp> {
        let uaid = self.store.get_uaid()?;
        let auth = self.store.get_auth()?;
        let reg_id = self.store.get_registration_id()?;
        connect(self.config.clone(), uaid, auth, reg_id)
    }
    // XXX: make these trait methods
    pub fn subscribe(
        &mut self,
        channel_id: &str,
        scope: &str,
        server_key: Option<&str>,
    ) -> Result<SubscriptionResponse> {
        // While potentially an error, a misconfigured system may use "" as
        // an application key. In that case, we drop the application key.
        let server_key = if let Some("") = server_key {
            None
        } else {
            server_key
        };
        // Don't fetch the subscription from the server if we've already got one.
        if let Some(record) = self.store.get_record(channel_id)? {
            let uaid = self.store.get_uaid()?.ok_or_else(|| {
                // should be impossible - we should delete all records when we lose our uiad.
                PushError::StorageError("DB has a subscription but no UAID".to_string())
            })?;
            log::debug!("returning existing subscription for '{}'", scope);
            return Ok((
                RegisterResponse {
                    uaid,
                    channel_id: record.channel_id,
                    endpoint: record.endpoint,
                    secret: self.store.get_auth()?,
                },
                Key::deserialize(&record.key)?,
            )
                .into());
        }

        let mut conn = self.make_connection()?;
        let info = conn.subscribe(channel_id, server_key)?;
        log::debug!("server returned subscription info: {:?}", info);
        // If our uaid has changed, or this is the first subscription we have made, all existing
        // records must die - but we can keep this one!
        let is_new_uaid = match self.store.get_uaid()? {
            Some(old_uaid) => old_uaid != info.uaid,
            None => true,
        };
        if is_new_uaid {
            // apparently the uaid changing but not getting a new secret guarantees we will be
            // unable to decrypt payloads. This should be impossible, so we could argue an
            // assertion makes more sense so it makes unmistakable noise, but for now we just Err.
            let new_auth = match info.secret {
                Some(ref secret) => secret,
                None => {
                    return Err(PushError::GeneralError(
                        "Server gave us a new uaid but no secret?".to_string(),
                    ))
                }
            };
            log::info!(
                "Got new new UAID of '{}' - deleting all existing records",
                info.uaid
            );
            self.store.delete_all_records()?;
            self.store.set_uaid(&info.uaid)?;
            self.store.set_auth(new_auth)?;
        }
        // store the channel_id => auth + subscription_key
        let subscription_key = if &self.config.sender_id == "test" {
            Crypto::test_key(
                "qJkxxWGVVxy7BKvraNY3hg8Gs-Y8qi0lRaXWJ3R3aJ8",
                "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50",
                "LsuUOBKVQRY6-l7_Ajo-Ag"
            )
        } else {
            Crypto::generate_key()?
        };
        let mut record = crate::internal::storage::PushRecord::new(
            &info.channel_id,
            &info.endpoint,
            scope,
            subscription_key.clone(),
        );
        record.app_server_key = server_key.map(|v| v.to_owned());
        self.store.put_record(&record)?;
        log::debug!("subscribed OK");
        Ok((info, subscription_key).into())
    }

    pub fn unsubscribe(&mut self, channel_id: &str) -> Result<bool> {
        // TODO(teshaq): This should throw an error instead of return false
        // keeping this as false in the meantime while uniffing to not change behavior
        // markh: both branches below are broken in our v3 schema - someone may have subscribed,
        // we then discover the server lost our subs (causing us to delete the world), and the
        // consumer then tries to unsubscribe. The consumer hasn't done anything wrong! We should
        // store "requested subscriptions" separately from "actual subscriptions" and this dilemma
        // would go away - it's an error to unsubscribe from something never subscribed to, but
        // not because we lost it!
        if channel_id.is_empty() {
            return Ok(false);
        }
        let conn = self.make_connection()?;
        if conn.uaid.is_none() {
            return Err(PushError::GeneralError(
                "No subscriptions created yet.".into(),
            ));
        }
        conn.unsubscribe(channel_id)?;
        self.store.delete_record(channel_id)
    }

    pub fn unsubscribe_all(&mut self) -> Result<()> {
        // make connection before deleting, because deletion kills our uaid/auth meta.
        let mut conn = self.make_connection()?;
        self.store.delete_all_records()?;
        conn.unsubscribe_all()?;
        Ok(())
    }

    pub fn update(&mut self, new_token: &str) -> error::Result<bool> {
        self.store.set_registration_id(new_token)?;
        let mut conn = self.make_connection()?;
        // It's OK if we don't have a uaid yet - that means we can't have any subscriptions,
        // and we've saved our registration_id, so will use it on our first subscription.
        if conn.uaid.is_none() {
            log::info!(
                "saved the registration ID but not telling the server as we have no subs yet"
            );
            return Ok(false);
        }
        if !self.update_rate_limiter.check(&self.store) {
            return Ok(false);
        }
        if let Err(e) = conn.update(new_token) {
            match e {
                PushError::UAIDNotRecognizedError(_) => {
                    // Our subscriptions are dead, but for now, just let the existing mechanisms
                    // deal with that (eg, next `subscribe()` or `verify_connection()`)
                    log::info!("updating our token indicated our subscriptions are gone");
                }
                _ => return Err(e),
            }
        }
        Ok(true)
    }

    pub fn verify_connection(&mut self) -> Result<Vec<PushSubscriptionChanged>> {
        let mut conn = self.make_connection()?;
        let channels = self.store.get_channel_list()?;
        if conn.verify_connection(&channels)? {
            // Everything is fine, our subscriptions in the db match the remote server.
            return Ok(Vec::new());
        }

        let mut subscriptions: Vec<PushSubscriptionChanged> = Vec::new();
        for channel in channels {
            if let Some(record) = self.store.get_record_by_chid(&channel)? {
                subscriptions.push(record.into());
            }
        }
        // we wipe all existing subscriptions and the UAID if there is a mismatch; the next
        // `subscribe()` call will get a new UAID.
        self.store.delete_all_records()?;
        Ok(subscriptions)
    }

    pub fn decrypt(
        &self,
        chid: &str,
        body: &str,
        encoding: &str,
        salt: Option<&str>,
        dh: Option<&str>,
    ) -> Result<Vec<u8>> {
        let val = self
            .store
            .get_record(chid)
            .map_err(|e| PushError::StorageError(format!("{:?}", e)))?
            .ok_or_else(|| PushError::RecordNotFoundError(chid.to_owned()))?;
        let key = Key::deserialize(&val.key)?;
        Crypto::decrypt(&key, body, encoding, salt, dh)
            .map_err(|e| PushError::CryptoError(format!("{:?}", e)))
    }

    pub fn get_record_by_chid(&self, chid: &str) -> error::Result<Option<DispatchInfo>> {
        Ok(self.store.get_record_by_chid(chid)?.map(Into::into))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    const TEST_CHANNEL_ID: &str = "deadbeef00000000decafbad00000000";

    fn get_test_manager() -> Result<PushManager> {
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            ..Default::default()
        };
        let pm = PushManager::new(test_config)?;
        pm.store.set_registration_id("native-id")?;
        Ok(pm)
    }
    #[test]
    fn basic() -> Result<()> {
        let mut pm = get_test_manager()?;
        let resp = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a subsequent request for the same channel ID returns the same subscription
        let resp2 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        assert_eq!(
            Some("LsuUOBKVQRY6-l7_Ajo-Ag".to_owned()),
            pm.store.get_auth()?
        );
        assert_eq!(
            resp.subscription_info.endpoint,
            resp2.subscription_info.endpoint
        );
        assert_eq!(resp.subscription_info.keys, resp2.subscription_info.keys);
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
        let mut pm = get_test_manager()?;
        let resp = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        let key_info = resp.subscription_info.keys;
        let remote_pub = base64::decode_config(&key_info.p256dh, base64::URL_SAFE_NO_PAD).unwrap();
        let auth = base64::decode_config(&key_info.auth, base64::URL_SAFE_NO_PAD).unwrap();
        // Act like a subscription provider, so create a "local" key to encrypt the data
        let ciphertext = ece::encrypt(&remote_pub, &auth, data_string).unwrap();
        let body = base64::encode_config(&ciphertext, base64::URL_SAFE_NO_PAD);

        let result = pm
            .decrypt(&resp.channel_id, &body, "aes128gcm", None, None)
            .unwrap();
        assert_eq!(
            serde_json::to_string(&data_string.to_vec()).unwrap(),
            serde_json::to_string(&result).unwrap(),
        );
        Ok(())
    }

    #[test]
    fn test_wipe_uaid() -> Result<()> {
        let mut pm = get_test_manager()?;
        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(
            pm.store.get_uaid()?.unwrap(),
            "abad1d3a00000000aabbccdd00000000"
        );
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        let unsubscribed_channels = pm.verify_connection()?;
        assert_eq!(unsubscribed_channels.len(), 1);
        assert_eq!(unsubscribed_channels[0].channel_id, TEST_CHANNEL_ID);
        // since verify_connection failed,
        // we wipe the uaid and all associated records from our store
        assert!(pm.store.get_uaid()?.is_none());
        assert!(pm.store.get_record(TEST_CHANNEL_ID)?.is_none());

        // we now check that a new subscription will cause us to
        // re-generate a uaid and store it in our store
        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that the uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(
            pm.store.get_uaid()?.unwrap(),
            "abad1d3a00000000aabbccdd00000000"
        );
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        Ok(())
    }
}
