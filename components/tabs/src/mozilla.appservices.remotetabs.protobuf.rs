#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientTabs {
    #[prost(string, required, tag="1")]
    pub client_id: std::string::String,
    #[prost(message, repeated, tag="2")]
    pub remote_tabs: ::std::vec::Vec<RemoteTab>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientsTabs {
    #[prost(message, repeated, tag="1")]
    pub clients_tabs: ::std::vec::Vec<ClientTabs>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RemoteTab {
    #[prost(string, required, tag="1")]
    pub title: std::string::String,
    #[prost(string, repeated, tag="2")]
    pub url_history: ::std::vec::Vec<std::string::String>,
    #[prost(string, optional, tag="3")]
    pub icon: ::std::option::Option<std::string::String>,
    #[prost(int64, required, tag="4")]
    pub last_used: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RemoteTabs {
    #[prost(message, repeated, tag="1")]
    pub remote_tabs: ::std::vec::Vec<RemoteTab>,
}
