/* Handle Push data storage
 */
extern crate crypto;

use openssl::ec::EcKey;
use openssl::pkey::Private;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crypto::Key;

pub type ChannelID = String;

#[derive(Clone, Debug, PartialEq)]
pub struct PushRecord {
    // Endpoint provided from the push server
    pub endpoint: String,

    // Designation label provided by the subscribing service
    pub designator: String,

    // List of origin Host attributes.
    pub origin_attributes: HashMap<String, String>,

    // Number of pushes for this record
    pub push_count: u8,

    // Last push rec'vd
    pub last_push: u64,

    // Private EC Prime256v1 key info. (Public key can be derived from this)
    pub private_key: Vec<u8>,

    // Push Server auth_secret
    pub auth_secret: String,

    // Is this as priviledged system record
    pub system_record: bool,

    // VAPID public key to restrict subscription updates for only those that sign
    // using the private VAPID key.
    pub app_server_key: Option<String>,

    // List of the most recent message IDs from the server.
    pub recent_message_ids: Vec<String>,

    // Time this subscription was created.
    pub ctime: u64,

    // Max quota count for sub
    pub quota: u8,

    // (if this is a bridged connection (e.g. on Android), this is the native OS Push ID)
    pub native_id: Option<String>,
}

fn now_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

impl PushRecord {
    fn increment(&mut self) -> Result<Self, StorageError> {
        self.push_count += 1;
        self.last_push = now_u64();
        // TODO check for quotas, etc
        // write to storage.
        Ok(self.clone())
    }
}

//TODO: Add broadcasts storage

pub struct StorageError;

pub trait Storage {
    // Connect to the storage system
    // fn connect<S: Storage>() -> S;

    // Generate a Push Record from the Subscription info, which has the endpoint,
    // encryption keys, etc.
    fn create_record(
        &self,
        uaid: &str,
        chid: &str,
        origin_attributes: HashMap<String, String>,
        endpoint: &str,
        auth: &str,
        private_key: &Key,
        system_record: bool,
    ) -> PushRecord;
    fn get_record(&self, uaid: &str, chid: &str) -> Option<PushRecord>;
    fn put_record(&self, uaid: &str, chid: &str, record: &PushRecord)
        -> Result<bool, StorageError>;
    fn purge(&self, uaid: &str, chid: Option<&str>) -> Result<bool, StorageError>;

    fn generate_channel_id(&self) -> String;
}

pub struct Store;

impl Store {
    fn connect() -> impl Storage {
        Store
    }
}

// TODO: Fill this out (pretty skeletal)
impl Storage for Store {
    fn create_record(
        &self,
        uaid: &str,
        chid: &str,
        origin_attributes: HashMap<String, String>,
        endpoint: &str,
        server_auth: &str,
        private_key: &Key,
        system_record: bool,
    ) -> PushRecord {
        // TODO: fill this out properly
        PushRecord {
            endpoint: String::from(endpoint),
            designator: String::from(chid),
            origin_attributes: origin_attributes.clone(),
            push_count: 0,
            last_push: 0,
            private_key: private_key.serialize().unwrap(),
            auth_secret: server_auth.to_string(),
            system_record: false,
            app_server_key: None,
            recent_message_ids: Vec::new(),
            // do we need sub second resolution?
            ctime: now_u64(),
            quota: 0,
            native_id: None,
        }
    }

    fn get_record(&self, uaid: &str, chid: &str) -> Option<PushRecord> {
        None
    }

    fn put_record(
        &self,
        uaid: &str,
        chid: &str,
        record: &PushRecord,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    fn purge(&self, uaid: &str, chid: Option<&str>) -> Result<bool, StorageError> {
        Ok(false)
    }

    fn generate_channel_id(&self) -> String {
        String::from("deadbeef00000000decafbad00000000")
    }
}
