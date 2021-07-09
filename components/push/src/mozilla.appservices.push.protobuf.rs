#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DispatchInfo {
    #[prost(string, required, tag="1")]
    pub uaid: ::prost::alloc::string::String,
    #[prost(string, required, tag="2")]
    pub scope: ::prost::alloc::string::String,
    #[prost(string, required, tag="3")]
    pub endpoint: ::prost::alloc::string::String,
    #[prost(string, optional, tag="4")]
    pub app_server_key: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyInfo {
    #[prost(string, required, tag="1")]
    pub auth: ::prost::alloc::string::String,
    #[prost(string, required, tag="2")]
    pub p256dh: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionInfo {
    #[prost(string, required, tag="1")]
    pub endpoint: ::prost::alloc::string::String,
    #[prost(message, required, tag="2")]
    pub keys: KeyInfo,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscriptionResponse {
    #[prost(string, required, tag="1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(message, required, tag="2")]
    pub subscription_info: SubscriptionInfo,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PushSubscriptionChanged {
    #[prost(string, required, tag="1")]
    pub channel_id: ::prost::alloc::string::String,
    #[prost(string, required, tag="2")]
    pub scope: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PushSubscriptionsChanged {
    #[prost(message, repeated, tag="1")]
    pub subs: ::prost::alloc::vec::Vec<PushSubscriptionChanged>,
}
