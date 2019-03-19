#![allow(unknown_lints)]

#[derive(Clone, Debug)]
pub struct PushConfiguration {
    // host name:port
    pub server_host: String,

    // connection protocol (for direct connections "wss")
    pub socket_protocol: Option<String>,

    // http protocol (for mobile, bridged connections "https")
    pub http_protocol: Option<String>,

    // bridge protocol ("fcm")
    pub bridge_type: Option<String>,

    // Native OS registration ID value
    pub registration_id: Option<String>,

    // base64 encoded public VAPID key (to secure endpoint)
    pub vapid_key: Option<String>,

    // Service enabled flag
    pub enabled: bool,

    // How often to ping server (1800s)
    pub ping_interval: u64,

    // HTTP request timeout value (1s)
    pub request_timeout: u64,

    // Sender/Application ID value
    pub sender_id: String,

    // OS Path to the database
    pub database_path: Option<String>,
}

impl Default for PushConfiguration {
    fn default() -> PushConfiguration {
        PushConfiguration {
            server_host: String::from("push.services.mozilla.com"),
            // socket_protocol: String::from("wss"),
            socket_protocol: None,
            http_protocol: Some(String::from("https")),
            bridge_type: Some(String::from("fcm")),
            registration_id: Some(String::from("deafbeef00000000")),
            vapid_key: None,
            enabled: true,
            ping_interval: 1800,
            request_timeout: 1,
            sender_id: String::from(""),
            database_path: None,
        }
    }
}
