#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PasswordInfo {
    #[prost(string, required, tag="1")]
    pub id: std::string::String,
    #[prost(string, required, tag="2")]
    pub hostname: std::string::String,
    #[prost(string, required, tag="3")]
    pub password: std::string::String,
    #[prost(string, required, tag="4")]
    pub username: std::string::String,
    #[prost(string, optional, tag="5")]
    pub http_realm: ::std::option::Option<std::string::String>,
    #[prost(string, optional, tag="6")]
    pub form_submit_url: ::std::option::Option<std::string::String>,
    #[prost(string, required, tag="7")]
    pub username_field: std::string::String,
    #[prost(string, required, tag="8")]
    pub password_field: std::string::String,
    #[prost(int64, required, tag="9")]
    pub times_used: i64,
    #[prost(int64, required, tag="10")]
    pub time_created: i64,
    #[prost(int64, required, tag="11")]
    pub time_last_used: i64,
    #[prost(int64, required, tag="12")]
    pub time_password_changed: i64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PasswordInfos {
    #[prost(message, repeated, tag="1")]
    pub infos: ::std::vec::Vec<PasswordInfo>,
}
