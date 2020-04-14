#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncParams {
    #[prost(string, repeated, tag="1")]
    pub engines_to_sync: ::std::vec::Vec<std::string::String>,
    #[prost(bool, required, tag="2")]
    pub sync_all_engines: bool,
    #[prost(enumeration="SyncReason", required, tag="3")]
    pub reason: i32,
    #[prost(map="string, bool", tag="4")]
    pub engines_to_change_state: ::std::collections::HashMap<std::string::String, bool>,
    #[prost(string, optional, tag="5")]
    pub persisted_state: ::std::option::Option<std::string::String>,
    /// These conceptually are a nested type, but exposing them as such would add
    /// needless complexity to the FFI.
    #[prost(string, required, tag="6")]
    pub acct_key_id: std::string::String,
    #[prost(string, required, tag="7")]
    pub acct_access_token: std::string::String,
    #[prost(string, required, tag="8")]
    pub acct_tokenserver_url: std::string::String,
    #[prost(string, required, tag="9")]
    pub acct_sync_key: std::string::String,
    #[prost(string, required, tag="10")]
    pub fxa_device_id: std::string::String,
    #[prost(string, required, tag="11")]
    pub device_name: std::string::String,
    #[prost(enumeration="DeviceType", required, tag="12")]
    pub device_type: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncResult {
    #[prost(enumeration="ServiceStatus", required, tag="1")]
    pub status: i32,
    /// empty string used for 'no error'
    #[prost(map="string, string", tag="2")]
    pub results: ::std::collections::HashMap<std::string::String, std::string::String>,
    #[prost(string, repeated, tag="3")]
    pub declined: ::std::vec::Vec<std::string::String>,
    /// false if we didn't manage to check declined.
    #[prost(bool, required, tag="4")]
    pub have_declined: bool,
    #[prost(int64, optional, tag="5")]
    pub next_sync_allowed_at: ::std::option::Option<i64>,
    #[prost(string, required, tag="6")]
    pub persisted_state: std::string::String,
    #[prost(string, optional, tag="7")]
    pub telemetry_json: ::std::option::Option<std::string::String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SyncReason {
    Scheduled = 1,
    User = 2,
    PreSleep = 3,
    Startup = 4,
    EnabledChange = 5,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DeviceType {
    Desktop = 1,
    Mobile = 2,
    Tablet = 3,
    Vr = 4,
    Tv = 5,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ServiceStatus {
    Ok = 1,
    NetworkError = 2,
    ServiceError = 3,
    AuthError = 4,
    BackedOff = 5,
    OtherError = 6,
}
