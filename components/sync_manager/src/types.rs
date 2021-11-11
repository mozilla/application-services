use std::collections::HashMap;
use std::time::SystemTime;
use sync15::clients::DeviceType;

#[derive(Debug)]
pub struct SyncParams {
    pub reason: SyncReason,
    pub engines: SyncEngineSelection,
    pub enabled_changes: HashMap<String, bool>,
    pub local_encryption_keys: HashMap<String, String>,
    pub auth_info: SyncAuthInfo,
    pub persisted_state: Option<String>,
    pub device_settings: DeviceSettings,
}

#[derive(Debug)]
pub enum SyncReason {
    Scheduled,
    User,
    PreSleep,
    Startup,
    EnabledChange,
}

#[derive(Debug)]
pub enum SyncEngineSelection {
    All,
    Some { engines: Vec<String> },
}

#[derive(Debug)]
pub struct SyncAuthInfo {
    pub kid: String,
    pub fxa_access_token: String,
    pub sync_key: String,
    pub tokenserver_url: String,
}

#[derive(Debug)]
pub struct DeviceSettings {
    pub fxa_device_id: String,
    pub name: String,
    pub kind: DeviceType,
}

#[derive(Debug)]
pub struct SyncResult {
    pub status: ServiceStatus,
    pub failures: HashMap<String, String>,
    pub successful: Vec<String>,
    pub persisted_state: String,
    pub declined: Option<Vec<String>>,
    pub next_sync_allowed_at: Option<SystemTime>,
    pub telemetry_json: Option<String>,
}

#[derive(Debug)]
pub enum ServiceStatus {
    Ok,
    NetworkError,
    ServiceError,
    AuthError,
    BackedOff,
    OtherError,
}
