#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DispatchInfo {
    #[prost(string, required, tag="1")]
    pub uaid: std::string::String,
    #[prost(string, required, tag="2")]
    pub scope: std::string::String,
    #[prost(string, required, tag="3")]
    pub endpoint: std::string::String,
    #[prost(string, optional, tag="4")]
    pub app_server_key: ::std::option::Option<std::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyInfo {
    #[prost(string, required, tag="1")]
    pub auth: std::string::String,
    #[prost(string, required, tag="2")]
    pub p256dh: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionInfo {
    #[prost(string, required, tag="1")]
    pub endpoint: std::string::String,
    #[prost(message, required, tag="2")]
    pub keys: KeyInfo,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionResponse {
    #[prost(string, required, tag="1")]
    pub channel_id: std::string::String,
    #[prost(message, required, tag="2")]
    pub subscription_info: SubscriptionInfo,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PushSubscriptionChanged {
    #[prost(string, required, tag="1")]
    pub channel_id: std::string::String,
    #[prost(string, required, tag="2")]
    pub scope: std::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PushSubscriptionsChanged {
    #[prost(message, repeated, tag="1")]
    pub subs: ::std::vec::Vec<PushSubscriptionChanged>,
}
