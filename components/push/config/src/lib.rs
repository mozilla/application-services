#[derive(Clone, Debug)]
pub struct PushConfiguration {
    pub server_host: String,
    pub socket_protocol: Option<String>,
    pub http_protocol: Option<String>,
    pub bridge_type: Option<String>,
    pub application_id: Option<String>,
    pub always_connect: bool,
    pub enabled: bool,
    pub ping_interval: u64,
    pub request_timeout: u64,
    pub sender_id: String,
}

impl Default for PushConfiguration {
    fn default() -> PushConfiguration {
        PushConfiguration {
            server_host: String::from("push.services.mozilla.com"),
            // socket_protocol: String::from("wss"),
            socket_protocol: None,
            http_protocol: Some(String::from("https")),
            bridge_type: None,
            application_id: None,
            always_connect: true,
            enabled: true,
            ping_interval: 1800,
            request_timeout: 1,
            sender_id: String::from(""),
        }
    }
}
