/* Handle external Push Subscription Requests.
 * "priviledged" system calls may require additional handling and should be flagged as such.
 */

#![allow(unknown_lints)]

extern crate serde_json;

extern crate communications;
extern crate crypto;
extern crate storage;

use std::collections::HashMap;

use communications::{connect, ConnectHttp, Connection, RegisterResponse};
use config::PushConfiguration;
use crypto::{Crypto, Cryptography, Key};
use storage::{Storage, Store};

use push_errors::{self as error, ErrorKind, Result};

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
        log::debug!("UAID is {:?}", &uaid);
        Ok(pm)
    }

    // XXX: make these trait methods
    // XXX: should be called subscribe?
    pub fn subscribe(&mut self, channel_id: &str, scope: &str) -> Result<(RegisterResponse, Key)> {
        //let key = self.config.vapid_key;
        let reg_token = self.config.registration_id.clone().unwrap();
        let subscription_key: Key;
        let info = self.conn.subscribe(channel_id)?;
        if &self.config.sender_id == "test" {
            subscription_key = Crypto::test_key(
                "MHcCAQEEIKiZMcVhlVccuwSr62jWN4YPBrPmPKotJUWl1id0d2ifoAoGCCqGSM49AwEHoUQDQgAEFwl1-zUa0zLKYVO23LqUgZZEVesS0k_jQN_SA69ENHgPwIpWCoTq-VhHu0JiSwhF0oPUzEM-FBWYoufO6J97nQ",
                "BBcJdfs1GtMyymFTtty6lIGWRFXrEtJP40Df0gOvRDR4D8CKVgqE6vlYR7tCYksIRdKD1MxDPhQVmKLnzuife50",
                "LsuUOBKVQRY6-l7_Ajo-Ag"
            )
        } else {
            subscription_key = Crypto::generate_key().unwrap();
        }
        // store the channelid => auth + subscription_key
        let mut record = storage::PushRecord::new(
            &info.uaid,
            &channel_id,
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
        }
        if self.store.get_meta("auth")?.is_none() {
            if let Some(secret) = &info.secret {
                self.store.set_meta("auth", &secret)?;
            }
        }
        Ok((info, subscription_key))
    }

    // XXX: maybe -> Result<()> instead
    // XXX: maybe handle channel_id None case separately?
    pub fn unsubscribe(&self, channel_id: Option<&str>) -> Result<bool> {
        if self.conn.uaid.is_none() {
            return Err(ErrorKind::GeneralError("No subscriptions created yet.".into()).into());
        }
        let result = self.conn.unsubscribe(channel_id)?;
        self.store
            .delete_record(self.conn.uaid.as_ref().unwrap(), channel_id.unwrap())?;
        Ok(result)
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
        self.conn.verify_connection(&channels)
    }

    pub fn decrypt(
        &self,
        uaid: &str,
        chid: &str,
        body: &str,
        encoding: &str,
        dh: Option<&str>,
        salt: Option<&str>,
    ) -> Result<String> {
        match self.store.get_record(&uaid, chid) {
            Err(e) => Err(ErrorKind::StorageError(format!("{:?}", e)).into()),
            Ok(v) => {
                if let Some(val) = v {
                    let key = Key::deserialize(val.key)?;
                    return match Crypto::decrypt(&key, body, encoding, salt, dh) {
                        Err(e) => Err(ErrorKind::EncryptionError(format!("{:?}", e)).into()),
                        Ok(v) => serde_json::to_string(&v)
                            .map_err(|e| ErrorKind::TranscodingError(format!("{:?}", e)).into()),
                    };
                };
                Err(ErrorKind::StorageError(format!(
                    "No record for uaid:chid {:?}:{:?}",
                    self.conn.uaid, chid
                ))
                .into())
            }
        }
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

    pub fn get_record_by_chid(&self, chid: &str) -> error::Result<Option<storage::PushRecord>> {
        self.store.get_record_by_chid(chid)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    //use serde_json::json;

    // use crypto::{get_bytes, Key};

    /*
    const DUMMY_CHID: &str = "deadbeef00000000decafbad00000000";
    const DUMMY_UAID: &str = "abad1dea00000000aabbccdd00000000";
    // Local test SENDER_ID
    const SENDER_ID: &str = "308358850242";
    const SECRET: &str = "SuP3rS1kRet";
    */

    #[test]
    fn basic() -> Result<()> {
        let _pm = PushManager::new(Default::default())?;
        //pm.subscribe(DUMMY_CHID, "http://example.com/test-scope")?;
        //pm.unsubscribe(Some(DUMMY_CHID))?;
        Ok(())
    }
}
