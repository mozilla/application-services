/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Main entrypoint for the push component, handles push subscriptions
//!
//! Exposes a struct [`PushManager`] that manages push subscriptions for a client
//!
//! The [`PushManager`] allows users to:
//! - Create new subscriptions persist their private keys and return a URL for sender to send encrypted payloads using a returned public key
//! - Delete existing subscriptions
//! - Update native tokens with autopush server
//! - routinely check subscriptions to make sure they are in a good state.

use std::collections::HashSet;

use crate::error::{self, PushError, Result};
use crate::internal::communications::{Connection, PersistedRateLimiter};
use crate::internal::config::PushConfiguration;
use crate::internal::crypto::KeyV1 as Key;
use crate::internal::storage::{PushRecord, Storage};
use crate::{
    DispatchInfo, KeyInfo, PushSubscriptionChanged, SubscriptionInfo, SubscriptionResponse,
};

use super::crypto::Cryptography;
const UPDATE_RATE_LIMITER_INTERVAL: u64 = 24 * 60 * 60; // 24 hours.
const UPDATE_RATE_LIMITER_MAX_CALLS: u16 = 500; // 500

impl From<Key> for KeyInfo {
    fn from(key: Key) -> Self {
        KeyInfo {
            auth: base64::encode_config(key.auth_secret(), base64::URL_SAFE_NO_PAD),
            p256dh: base64::encode_config(key.public_key(), base64::URL_SAFE_NO_PAD),
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

pub struct PushManager<Co, Cr, S> {
    _crypo: Cr,
    connection: Co,
    uaid: Option<String>,
    auth: Option<String>,
    registration_id: Option<String>,
    store: S,
    update_rate_limiter: PersistedRateLimiter,
    verify_connection_rate_limiter: PersistedRateLimiter,
}

impl<Co: Connection, Cr: Cryptography, S: Storage> PushManager<Co, Cr, S> {
    pub fn new(config: PushConfiguration) -> Result<Self> {
        let store = S::open(&config.database_path)?;
        let uaid = store.get_uaid()?;
        let auth = store.get_auth()?;
        let registration_id = store.get_registration_id()?;
        let verify_connection_rate_limiter = PersistedRateLimiter::new(
            "verify_connection",
            config
                .verify_connection_rate_limiter
                .unwrap_or(super::config::DEFAULT_VERIFY_CONNECTION_LIMITER_INTERVAL),
            1,
        );

        let update_rate_limiter = PersistedRateLimiter::new(
            "update_token",
            UPDATE_RATE_LIMITER_INTERVAL,
            UPDATE_RATE_LIMITER_MAX_CALLS,
        );

        Ok(Self {
            connection: Co::connect(config),
            _crypo: Default::default(),
            uaid,
            auth,
            registration_id,
            store,
            update_rate_limiter,
            verify_connection_rate_limiter,
        })
    }

    fn ensure_auth_pair(&self) -> Result<(&str, &str)> {
        if let (Some(uaid), Some(auth)) = (&self.uaid, &self.auth) {
            Ok((uaid, auth))
        } else {
            Err(PushError::GeneralError(
                "No subscriptions created yet.".into(),
            ))
        }
    }

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
            if self.uaid.is_none() {
                // should be impossible - we should delete all records when we lose our uiad.
                return Err(PushError::StorageError(
                    "DB has a subscription but no UAID".to_string(),
                ));
            }
            log::debug!("returning existing subscription for '{}'", scope);
            return Ok(SubscriptionResponse {
                channel_id: record.channel_id,
                subscription_info: SubscriptionInfo {
                    endpoint: record.endpoint,
                    keys: Key::deserialize(&record.key)?.into(),
                },
            });
        }

        let registration_id = self
            .registration_id
            .as_ref()
            .ok_or_else(|| PushError::CommunicationError("No native id".to_string()))?
            .clone();

        let subscription_response =
            self.impl_subscribe(channel_id, scope, &registration_id, server_key);

        if matches!(
            subscription_response,
            Err(PushError::AlreadyRegisteredError)
        ) {
            // somehow the server thinks we're registered, but we don't have the
            // record locally. So, we unsubscribe from the server, then try to re-subscribe
            self.unsubscribe(channel_id)?;
            self.impl_subscribe(channel_id, scope, &registration_id, server_key)
        } else {
            subscription_response
        }
    }

    pub fn unsubscribe(&mut self, channel_id: &str) -> Result<()> {
        let (uaid, auth) = self.ensure_auth_pair()?;
        self.connection.unsubscribe(channel_id, uaid, auth)?;
        self.store.delete_record(channel_id)?;
        Ok(())
    }

    pub fn unsubscribe_all(&mut self) -> Result<()> {
        let (uaid, auth) = self.ensure_auth_pair()?;

        self.connection.unsubscribe_all(uaid, auth)?;
        self.wipe_local_registrations()?;
        Ok(())
    }

    pub fn update(&mut self, new_token: &str) -> error::Result<()> {
        if self.registration_id.as_deref() == Some(new_token) {
            // Already up to date!
            // if we haven't send it to the server yet, we will on the next subscribe!
            // if we have sent it to the server, no need to do so again. We will catch any issues
            // through the [`PushManager::verify_connection`] check
            return Ok(());
        }

        // It's OK if we don't have a uaid yet - that means we don't have any subscriptions,
        // let save our registration_id, so will use it on our first subscription.
        if self.uaid.is_none() {
            self.store.set_registration_id(new_token)?;
            self.registration_id = Some(new_token.to_string());
            log::info!(
                "saved the registration ID but not telling the server as we have no subs yet"
            );
            return Ok(());
        }

        if !self.update_rate_limiter.check(&self.store) {
            return Ok(());
        }

        let (uaid, auth) = self.ensure_auth_pair()?;

        if let Err(e) = self.connection.update(new_token, uaid, auth) {
            match e {
                PushError::UAIDNotRecognizedError(_) => {
                    // Our subscriptions are dead, but for now, just let the existing mechanisms
                    // deal with that (eg, next `subscribe()` or `verify_connection()`)
                    log::info!("updating our token indicated our subscriptions are gone");
                }
                _ => return Err(e),
            }
        }

        self.store.set_registration_id(new_token)?;
        self.registration_id = Some(new_token.to_string());
        Ok(())
    }

    pub fn verify_connection(
        &mut self,
        force_verify: bool,
    ) -> Result<Vec<PushSubscriptionChanged>> {
        if force_verify {
            self.verify_connection_rate_limiter.reset(&self.store);
        }
        if !self.verify_connection_rate_limiter.check(&self.store) {
            return Ok(vec![]);
        }
        let channels = self.store.get_channel_list()?;
        let (uaid, auth) = self.ensure_auth_pair()?;

        let local_channels: HashSet<String> = channels.into_iter().collect();
        let remote_channels = match self.connection.channel_list(uaid, auth) {
            Ok(v) => Some(HashSet::from_iter(v)),
            Err(e) => match e {
                PushError::UAIDNotRecognizedError(_) => {
                    // We do not unsubscribe, because the server already lost our UAID
                    None
                }
                _ => return Err(e),
            },
        };

        // verify both lists match. Either side could have lost its mind.
        match remote_channels {
            // Everything is OK! Lets return early
            Some(channels) if channels == local_channels => return Ok(Vec::new()),
            Some(_) => {
                log::info!("verify_connection found a mismatch - unsubscribing");
                // Unsubscribe all the channels (just to be sure and avoid a loop).
                self.connection.unsubscribe_all(uaid, auth)?;
            }
            // Means the server lost our UAID, lets not unsubscribe,
            // as that operation will fail
            None => (),
        };

        let mut subscriptions: Vec<PushSubscriptionChanged> = Vec::new();
        for channel in local_channels {
            if let Some(record) = self.store.get_record_by_chid(&channel)? {
                subscriptions.push(record.into());
            }
        }
        // we wipe all existing subscriptions and the UAID if there is a mismatch; the next
        // `subscribe()` call will get a new UAID.
        self.wipe_local_registrations()?;
        Ok(subscriptions)
    }

    pub fn decrypt(
        &self,
        chid: &str,
        body: &str,
        encoding: &str,
        salt: &str,
        dh: &str,
    ) -> Result<Vec<u8>> {
        let val = self
            .store
            .get_record(chid)?
            .ok_or_else(|| PushError::RecordNotFoundError(chid.to_owned()))?;
        let key = Key::deserialize(&val.key)?;
        Cr::decrypt(&key, body, encoding, salt, dh)
    }

    pub fn get_record_by_chid(&self, chid: &str) -> error::Result<Option<DispatchInfo>> {
        Ok(self.store.get_record_by_chid(chid)?.map(Into::into))
    }

    fn wipe_local_registrations(&mut self) -> error::Result<()> {
        self.store.delete_all_records()?;
        self.auth = None;
        self.uaid = None;
        Ok(())
    }

    fn impl_subscribe(
        &mut self,
        channel_id: &str,
        scope: &str,
        registration_id: &str,
        server_key: Option<&str>,
    ) -> error::Result<SubscriptionResponse> {
        if let (Some(uaid), Some(auth)) = (&self.uaid, &self.auth) {
            self.subscribe_with_uaid(channel_id, scope, uaid, auth, registration_id, server_key)
        } else {
            self.register(channel_id, scope, registration_id, server_key)
        }
    }

    fn subscribe_with_uaid(
        &self,
        channel_id: &str,
        scope: &str,
        uaid: &str,
        auth: &str,
        registration_id: &str,
        app_server_key: Option<&str>,
    ) -> error::Result<SubscriptionResponse> {
        let app_server_key = app_server_key.map(|v| v.to_owned());

        let subscription_response =
            self.connection
                .subscribe(channel_id, uaid, auth, registration_id, &app_server_key)?;
        let subscription_key = Cr::generate_key()?;
        let mut record = crate::internal::storage::PushRecord::new(
            &subscription_response.channel_id,
            &subscription_response.endpoint,
            scope,
            subscription_key.clone(),
        )?;
        record.app_server_key = app_server_key;
        self.store.put_record(&record)?;
        log::debug!("subscribed OK");
        Ok(SubscriptionResponse {
            channel_id: subscription_response.channel_id,
            subscription_info: SubscriptionInfo {
                endpoint: subscription_response.endpoint,
                keys: subscription_key.into(),
            },
        })
    }

    fn register(
        &mut self,
        channel_id: &str,
        scope: &str,
        registration_id: &str,
        app_server_key: Option<&str>,
    ) -> error::Result<SubscriptionResponse> {
        let app_server_key = app_server_key.map(|v| v.to_owned());
        let register_response =
            self.connection
                .register(channel_id, registration_id, &app_server_key)?;
        // Registration successful! Before we return our registration, lets save our uaid and auth
        self.store.set_uaid(&register_response.uaid)?;
        self.store.set_auth(&register_response.secret)?;
        self.uaid = Some(register_response.uaid.clone());
        self.auth = Some(register_response.secret.clone());

        let subscription_key = Cr::generate_key()?;
        let mut record = crate::internal::storage::PushRecord::new(
            &register_response.channel_id,
            &register_response.endpoint,
            scope,
            subscription_key.clone(),
        )?;
        record.app_server_key = app_server_key;
        self.store.put_record(&record)?;
        log::debug!("subscribed OK");
        Ok(SubscriptionResponse {
            channel_id: register_response.channel_id,
            subscription_info: SubscriptionInfo {
                endpoint: register_response.endpoint,
                keys: subscription_key.into(),
            },
        })
    }
}

#[cfg(test)]
mod test {
    use mockall::predicate::eq;
    use rc_crypto::ece::{self, EcKeyComponents};

    use crate::internal::{
        communications::{MockConnection, RegisterResponse, SubscribeResponse},
        crypto::MockCryptography,
    };

    use super::*;
    use lazy_static::lazy_static;
    use std::sync::{Mutex, MutexGuard};

    use crate::Store;

    lazy_static! {
        static ref MTX: Mutex<()> = Mutex::new(());
    }

    // we need to run our tests in sequence. The tests mock static
    // methods. Mocked static methods are global are susceptible to data races
    // see: https://docs.rs/mockall/latest/mockall/#static-methods
    fn get_lock(m: &'static Mutex<()>) -> MutexGuard<'static, ()> {
        match m.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    const TEST_UAID: &str = "abad1d3a00000000aabbccdd00000000";
    const DATA: &[u8] = b"Mary had a little lamb, with some nice mint jelly";
    const TEST_CHANNEL_ID: &str = "deadbeef00000000decafbad00000000";
    const TEST_CHANNEL_ID2: &str = "decafbad00000000deadbeef00000000";

    const PRIV_KEY_D: &str = "qJkxxWGVVxy7BKvraNY3hg8Gs-Y8qi0lRaXWJ3R3aJ8";
    // The auth token
    const TEST_AUTH: &str = "LsuUOBKVQRY6-l7_Ajo-Ag";
    // This would be the public key sent to the subscription service.
    const PUB_KEY_RAW: &str =
        "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50";

    const ONE_DAY_AND_ONE_SECOND: u64 = (24 * 60 * 60) + 1;

    fn get_test_manager() -> Result<PushManager<MockConnection, MockCryptography, Store>> {
        let test_config = PushConfiguration {
            sender_id: "test".to_owned(),
            ..Default::default()
        };

        let mut pm: PushManager<MockConnection, MockCryptography, Store> =
            PushManager::new(test_config)?;
        pm.store.set_registration_id("native-id")?;
        pm.registration_id = Some("native-id".to_string());
        Ok(pm)
    }
    #[test]
    fn basic() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        let resp = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a subsequent request for the same channel ID returns the same subscription
        let resp2 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        assert_eq!(Some(TEST_AUTH.to_owned()), pm.store.get_auth()?);
        assert_eq!(
            resp.subscription_info.endpoint,
            resp2.subscription_info.endpoint
        );
        assert_eq!(resp.subscription_info.keys, resp2.subscription_info.keys);

        pm.connection
            .expect_unsubscribe()
            .with(eq(TEST_CHANNEL_ID), eq(TEST_UAID), eq(TEST_AUTH))
            .times(2)
            .returning(|_, _, _| Ok(()));
        pm.connection
            .expect_unsubscribe_all()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _| Ok(()));

        pm.unsubscribe(TEST_CHANNEL_ID)?;
        // // It's already deleted, we still return an OK
        pm.unsubscribe(TEST_CHANNEL_ID)?;
        pm.unsubscribe_all()?;
        Ok(())
    }

    #[test]
    fn full() -> Result<()> {
        let _m = get_lock(&MTX);
        rc_crypto::ensure_initialized();
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());
        let data_string = b"Mary had a little lamb, with some nice mint jelly";
        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });

        let resp = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        let key_info = resp.subscription_info.keys;
        let remote_pub = base64::decode_config(&key_info.p256dh, base64::URL_SAFE_NO_PAD).unwrap();
        let auth = base64::decode_config(&key_info.auth, base64::URL_SAFE_NO_PAD).unwrap();
        // Act like a subscription provider, so create a "local" key to encrypt the data
        let ciphertext = ece::encrypt(&remote_pub, &auth, data_string).unwrap();
        let body = base64::encode_config(ciphertext, base64::URL_SAFE_NO_PAD);

        let decryp_ctx = MockCryptography::decrypt_context();
        let body_clone = body.clone();
        decryp_ctx
            .expect()
            .withf(move |key, ibody, encoding, dh, salt| {
                *key == Key {
                    p256key: EcKeyComponents::new(
                        base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                        base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
                    ),
                    auth: base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap(),
                } && ibody == body_clone
                    && encoding == "aes128gcm"
                    && dh.is_empty()
                    && salt.is_empty()
            })
            .returning(|_, _, _, _, _| Ok(data_string.to_vec()));
        pm.decrypt(&resp.channel_id, &body, "aes128gcm", "", "")
            .unwrap();
        Ok(())
    }

    #[test]
    fn test_aesgcm_decryption() -> Result<()> {
        let _m = get_lock(&MTX);
        rc_crypto::ensure_initialized();

        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;

        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        let resp = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        let key_info = resp.subscription_info.keys;
        let remote_pub = base64::decode_config(&key_info.p256dh, base64::URL_SAFE_NO_PAD).unwrap();
        let auth = base64::decode_config(&key_info.auth, base64::URL_SAFE_NO_PAD).unwrap();
        // Act like a subscription provider, so create a "local" key to encrypt the data
        let ciphertext = ece::encrypt(&remote_pub, &auth, DATA).unwrap();
        let body = base64::encode_config(ciphertext, base64::URL_SAFE_NO_PAD);

        let decryp_ctx = MockCryptography::decrypt_context();
        let body_clone = body.clone();
        decryp_ctx
            .expect()
            .withf(move |key, ibody, encoding, dh, salt| {
                *key == Key {
                    p256key: EcKeyComponents::new(
                        base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                        base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
                    ),
                    auth: base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap(),
                } && ibody == body_clone
                    && encoding == "aesgcm"
                    && dh.is_empty()
                    && salt.is_empty()
            })
            .returning(|_, _, _, _, _| Ok(DATA.to_vec()));
        pm.decrypt(&resp.channel_id, &body, "aesgcm", "", "")
            .unwrap();
        Ok(())
    }

    #[test]
    fn test_get_record_by_chid() -> Result<()> {
        let _m = get_lock(&MTX);
        rc_crypto::ensure_initialized();

        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;

        pm.connection
            .expect_register()
            .with(
                eq(TEST_CHANNEL_ID),
                eq("native-id"),
                eq(Some("server_key!".to_string())),
            )
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", Some("server_key!"))?;

        let record = pm.get_record_by_chid(TEST_CHANNEL_ID).unwrap().unwrap();
        assert_eq!(
            record.endpoint,
            "https://example.com/dummy-endpoint".to_string()
        );
        assert_eq!(record.app_server_key, Some("server_key!".to_string()));
        assert_eq!(record.scope, "test-scope");
        Ok(())
    }

    #[test]
    fn test_duplicate_subscription_requests() -> Result<()> {
        let _m = get_lock(&MTX);
        rc_crypto::ensure_initialized();

        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;

        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1) // only once, second time we'll hit cache!
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        let sub_1 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        let sub_2 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        assert_eq!(sub_1, sub_2);
        Ok(())
    }
    #[test]
    fn test_verify_wipe_uaid_if_mismatch() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(2)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        pm.connection
            .expect_channel_list()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _| Ok(vec![TEST_CHANNEL_ID2.to_string()]));

        pm.connection
            .expect_unsubscribe_all()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _| Ok(()));
        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(pm.store.get_uaid()?.unwrap(), TEST_UAID);
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        let unsubscribed_channels = pm.verify_connection(false)?;
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
        assert_eq!(pm.store.get_uaid()?.unwrap(), TEST_UAID);
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        Ok(())
    }

    #[test]
    fn test_verify_server_lost_uaid_not_error() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        pm.connection
            .expect_channel_list()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _| {
                Err(PushError::UAIDNotRecognizedError(
                    "Couldn't find uaid".to_string(),
                ))
            });

        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(pm.store.get_uaid()?.unwrap(), TEST_UAID);
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        let unsubscribed_channels = pm.verify_connection(false)?;
        assert_eq!(unsubscribed_channels.len(), 1);
        assert_eq!(unsubscribed_channels[0].channel_id, TEST_CHANNEL_ID);
        // since verify_connection failed,
        // we wipe the uaid and all associated records from our store
        assert!(pm.store.get_uaid()?.is_none());
        assert!(pm.store.get_record(TEST_CHANNEL_ID)?.is_none());
        Ok(())
    }

    #[test]
    fn test_verify_server_hard_error() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        pm.connection
            .expect_channel_list()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _| {
                Err(PushError::CommunicationError(
                    "Unrecoverable error".to_string(),
                ))
            });

        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // verify that a uaid got added to our store and
        // that there is a record associated with the channel ID provided
        assert_eq!(pm.store.get_uaid()?.unwrap(), TEST_UAID);
        assert_eq!(
            pm.store.get_record(TEST_CHANNEL_ID)?.unwrap().channel_id,
            TEST_CHANNEL_ID
        );
        let err = pm.verify_connection(false).unwrap_err();

        // the same error got propagated
        assert!(matches!(err, PushError::CommunicationError(_)));
        Ok(())
    }

    #[test]
    fn test_second_subscribe_hits_subscribe_endpoint() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        pm.connection
            .expect_subscribe()
            .with(
                eq(TEST_CHANNEL_ID2),
                eq(TEST_UAID),
                eq(TEST_AUTH),
                eq("native-id"),
                eq(None),
            )
            .times(1)
            .returning(|_, _, _, _, _| {
                Ok(SubscribeResponse {
                    channel_id: TEST_CHANNEL_ID2.to_string(),
                    endpoint: "https://example.com/different-dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });

        let resp_1 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        let resp_2 = pm.subscribe(TEST_CHANNEL_ID2, "another-scope", None)?;
        assert_eq!(
            resp_1.subscription_info.endpoint,
            "https://example.com/dummy-endpoint"
        );
        assert_eq!(
            resp_2.subscription_info.endpoint,
            "https://example.com/different-dummy-endpoint"
        );
        Ok(())
    }

    #[test]
    fn test_server_returns_already_registered() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        pm.connection
            .expect_subscribe()
            .with(
                eq(TEST_CHANNEL_ID),
                eq(TEST_UAID),
                eq(TEST_AUTH),
                eq("native-id"),
                eq(None),
            )
            .times(1)
            .returning(|_, _, _, _, _| Err(PushError::AlreadyRegisteredError));
        pm.connection
            .expect_subscribe()
            .with(
                eq(TEST_CHANNEL_ID),
                eq(TEST_UAID),
                eq(TEST_AUTH),
                eq("native-id"),
                eq(None),
            )
            .times(1)
            .returning(|_, _, _, _, _| {
                Ok(SubscribeResponse {
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    endpoint: "https://example.com/different-dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });

        pm.connection
            .expect_unsubscribe()
            .with(eq(TEST_CHANNEL_ID), eq(TEST_UAID), eq(TEST_AUTH))
            .times(1)
            .returning(|_, _, _| Ok(()));

        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });

        // This call will trigger both registration and caching of the record
        let resp_1 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        // We wipe all records to mock the case where the server thinks there is a registration already
        // that the client lost
        pm.store.delete_all_records()?;
        // The call will trigger subscription, and mocks a server returning an `AlreadyRegistered` error
        let resp_2 = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        assert_eq!(
            resp_1.subscription_info.endpoint,
            "https://example.com/dummy-endpoint"
        );
        assert_eq!(
            resp_2.subscription_info.endpoint,
            "https://example.com/different-dummy-endpoint"
        );
        Ok(())
    }

    #[test]
    fn test_verify_connection_rate_limiter() -> Result<()> {
        let _m = get_lock(&MTX);
        let ctx = MockConnection::connect_context();
        ctx.expect().returning(|_| Default::default());

        let mut pm = get_test_manager()?;
        pm.connection
            .expect_register()
            .with(eq(TEST_CHANNEL_ID), eq("native-id"), eq(None))
            .times(1)
            .returning(|_, _, _| {
                Ok(RegisterResponse {
                    uaid: TEST_UAID.to_string(),
                    channel_id: TEST_CHANNEL_ID.to_string(),
                    secret: TEST_AUTH.to_string(),
                    endpoint: "https://example.com/dummy-endpoint".to_string(),
                    sender_id: Some("test".to_string()),
                })
            });
        let crypto_ctx = MockCryptography::generate_key_context();
        crypto_ctx.expect().returning(|| {
            let components = EcKeyComponents::new(
                base64::decode_config(PRIV_KEY_D, base64::URL_SAFE_NO_PAD).unwrap(),
                base64::decode_config(PUB_KEY_RAW, base64::URL_SAFE_NO_PAD).unwrap(),
            );
            let auth = base64::decode_config(TEST_AUTH, base64::URL_SAFE_NO_PAD).unwrap();
            Ok(Key {
                p256key: components,
                auth,
            })
        });
        let _ = pm.subscribe(TEST_CHANNEL_ID, "test-scope", None)?;
        pm.connection
            .expect_channel_list()
            .with(eq(TEST_UAID), eq(TEST_AUTH))
            .times(3)
            .returning(|_, _| Ok(vec![TEST_CHANNEL_ID.to_string()]));
        let _ = pm.verify_connection(false)?;
        let (_, count) = pm.verify_connection_rate_limiter.get_counters(&pm.store);
        assert_eq!(count, 1);
        let _ = pm.verify_connection(false)?;
        let (timestamp, count) = pm.verify_connection_rate_limiter.get_counters(&pm.store);

        assert_eq!(count, 2);

        pm.verify_connection_rate_limiter.persist_counters(
            &pm.store,
            timestamp - ONE_DAY_AND_ONE_SECOND,
            count,
        );

        let _ = pm.verify_connection(false)?;
        let (_, count) = pm.verify_connection_rate_limiter.get_counters(&pm.store);
        assert_eq!(count, 1);

        // Even though a day hasn't passed, we passed `true` to force verify
        // so the counter is now reset
        let _ = pm.verify_connection(true)?;
        let (_, count) = pm.verify_connection_rate_limiter.get_counters(&pm.store);
        assert_eq!(count, 1);

        Ok(())
    }
}
