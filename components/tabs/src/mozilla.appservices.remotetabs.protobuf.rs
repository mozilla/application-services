#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientTabs {
    #[prost(string, required, tag="1")]
    pub client_id: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="2")]
    pub remote_tabs: ::prost::alloc::vec::Vec<RemoteTab>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientsTabs {
    #[prost(message, repeated, tag="1")]
    pub clients_tabs: ::prost::alloc::vec::Vec<ClientTabs>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RemoteTab {
    #[prost(string, required, tag="1")]
    pub title: ::prost::alloc::string::String,
    #[prost(string, repeated, tag="2")]
    pub url_history: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub icon: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(int64, required, tag="4")]
    pub last_used: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RemoteTabs {
    #[prost(message, repeated, tag="1")]
    pub remote_tabs: ::prost::alloc::vec::Vec<RemoteTab>,
}
