extern crate bhttp;
extern crate ohttp;

use parking_lot::Mutex;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum OhttpError {
    #[error("Failed to fetch encryption key")]
    KeyFetchFailed,

    #[error("OHTTP key config is malformed")]
    MalformedKeyConfig,

    #[error("Unsupported OHTTP encryption algorithm")]
    UnsupportedKeyConfig,

    #[error("OhttpSession is in invalid state")]
    InvalidSession,

    #[error("Network errors communicating with Relay / Gateway")]
    RelayFailed,

    #[error("Cannot encode message as BHTTP/OHTTP")]
    CannotEncodeMessage,

    #[error("Cannot decode OHTTP/BHTTP message")]
    MalformedMessage,

    #[error("Duplicate HTTP response headers")]
    DuplicateHeaders,
}

#[derive(Default)]
enum ExchangeState {
    #[default]
    Invalid,
    Request(ohttp::ClientRequest),
    Response(ohttp::ClientResponse),
}

pub struct OhttpSession {
    state: Mutex<ExchangeState>,
}

pub struct OhttpResponse {
    status_code: u16,
    headers: HashMap<String, String>,
    payload: Vec<u8>,
}

/// Transform the headers from a BHTTP message into a HashMap for use from Swift
/// later. If there are duplicate errors, we currently raise an error.
fn headers_to_map(message: &bhttp::Message) -> Result<HashMap<String, String>, OhttpError> {
    let mut headers = HashMap::new();

    for field in message.header().iter() {
        if headers
            .insert(
                std::str::from_utf8(field.name())
                    .map_err(|_| OhttpError::MalformedMessage)?
                    .into(),
                std::str::from_utf8(field.value())
                    .map_err(|_| OhttpError::MalformedMessage)?
                    .into(),
            )
            .is_some()
        {
            return Err(OhttpError::DuplicateHeaders);
        }
    }

    Ok(headers)
}

impl OhttpSession {
    /// Create a new encryption session for use with specific key configuration
    pub fn new(config: &[u8]) -> Result<Self, OhttpError> {
        let request = ohttp::ClientRequest::new(config).map_err(|e| match e {
            ohttp::Error::Unsupported => OhttpError::UnsupportedKeyConfig,
            _ => OhttpError::MalformedKeyConfig,
        })?;

        let state = Mutex::new(ExchangeState::Request(request));
        Ok(OhttpSession { state })
    }

    /// Encode an HTTP request in Binary HTTP format and then encrypt it into an
    /// Oblivious HTTP request message.
    pub fn encapsulate(
        &self,
        method: &str,
        scheme: &str,
        server: &str,
        endpoint: &str,
        mut headers: HashMap<String, String>,
        payload: &[u8],
    ) -> Result<Vec<u8>, OhttpError> {
        let mut message =
            bhttp::Message::request(method.into(), scheme.into(), server.into(), endpoint.into());

        for (k, v) in headers.drain() {
            message.put_header(k, v);
        }

        message.write_content(payload);

        let mut encoded = vec![];
        message
            .write_bhttp(bhttp::Mode::KnownLength, &mut encoded)
            .map_err(|_| OhttpError::CannotEncodeMessage)?;

        let mut state = self.state.lock();
        let request = match std::mem::take(&mut *state) {
            ExchangeState::Request(request) => request,
            _ => return Err(OhttpError::InvalidSession),
        };
        let (capsule, response) = request
            .encapsulate(&encoded)
            .map_err(|_| OhttpError::CannotEncodeMessage)?;
        *state = ExchangeState::Response(response);

        Ok(capsule)
    }

    /// Decode an OHTTP response returned in response to a request encoded on
    /// this session.
    pub fn decapsulate(&self, encoded: &[u8]) -> Result<OhttpResponse, OhttpError> {
        let mut state = self.state.lock();
        let decoder = match std::mem::take(&mut *state) {
            ExchangeState::Response(response) => response,
            _ => return Err(OhttpError::InvalidSession),
        };
        let binary = decoder
            .decapsulate(encoded)
            .map_err(|_| OhttpError::MalformedMessage)?;

        let mut cursor = std::io::Cursor::new(binary);
        let message =
            bhttp::Message::read_bhttp(&mut cursor).map_err(|_| OhttpError::MalformedMessage)?;

        let headers = headers_to_map(&message)?;

        Ok(OhttpResponse {
            status_code: match message.control() {
                bhttp::ControlData::Response(sc) => *sc,
                _ => return Err(OhttpError::InvalidSession),
            },
            headers,
            payload: message.content().into(),
        })
    }
}

pub struct OhttpTestServer {
    server: Mutex<ohttp::Server>,
    state: Mutex<Option<ohttp::ServerResponse>>,
    config: Vec<u8>,
}

pub struct TestServerRequest {
    method: String,
    scheme: String,
    server: String,
    endpoint: String,
    headers: HashMap<String, String>,
    payload: Vec<u8>,
}

impl OhttpTestServer {
    /// Create a simple OHTTP server to decrypt and respond to OHTTP messages in
    /// testing. The key is randomly generated.
    fn new() -> Self {
        let key = ohttp::KeyConfig::new(
            0x01,
            ohttp::hpke::Kem::X25519Sha256,
            vec![ohttp::SymmetricSuite::new(
                ohttp::hpke::Kdf::HkdfSha256,
                ohttp::hpke::Aead::Aes128Gcm,
            )],
        )
        .unwrap();

        let config = key.encode().unwrap();
        let server = ohttp::Server::new(key).unwrap();

        OhttpTestServer {
            server: Mutex::new(server),
            state: Mutex::new(Option::None),
            config,
        }
    }

    /// Return a copy of the key config for clients to use.
    fn get_config(&self) -> Vec<u8> {
        self.config.clone()
    }

    /// Decode an OHTTP request message and return the cleartext contents. This
    /// also updates the internal server state so that a response message can be
    /// generated.
    fn receive(&self, message: &[u8]) -> Result<TestServerRequest, OhttpError> {
        let (encoded, response) = self
            .server
            .lock()
            .decapsulate(message)
            .map_err(|_| OhttpError::MalformedMessage)?;
        let mut cursor = std::io::Cursor::new(encoded);
        let message =
            bhttp::Message::read_bhttp(&mut cursor).map_err(|_| OhttpError::MalformedMessage)?;

        *self.state.lock() = Some(response);

        let headers = headers_to_map(&message)?;

        match message.control() {
            bhttp::ControlData::Request {
                method,
                scheme,
                authority,
                path,
            } => Ok(TestServerRequest {
                method: String::from_utf8_lossy(method).into(),
                scheme: String::from_utf8_lossy(scheme).into(),
                server: String::from_utf8_lossy(authority).into(),
                endpoint: String::from_utf8_lossy(path).into(),
                headers,
                payload: message.content().into(),
            }),
            _ => Err(OhttpError::MalformedMessage),
        }
    }

    /// Encode an OHTTP response keyed to the last message received.
    fn respond(&self, response: OhttpResponse) -> Result<Vec<u8>, OhttpError> {
        let state = self.state.lock().take().unwrap();

        let mut message = bhttp::Message::response(response.status_code);
        message.write_content(&response.payload);

        for (k, v) in response.headers {
            message.put_header(k, v);
        }

        let mut encoded = vec![];
        message
            .write_bhttp(bhttp::Mode::KnownLength, &mut encoded)
            .map_err(|_| OhttpError::CannotEncodeMessage)?;

        state
            .encapsulate(&encoded)
            .map_err(|_| OhttpError::CannotEncodeMessage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoke() {
        let server = OhttpTestServer::new();
        let config = server.get_config();

        let body: Vec<u8> = vec![0x00, 0x01, 0x02];
        let header = HashMap::from([
            ("Content-Type".into(), "application/octet-stream".into()),
            ("X-Header".into(), "value".into()),
        ]);

        let session = OhttpSession::new(&config).unwrap();
        let mut message = session
            .encapsulate("GET", "https", "example.com", "/api", header.clone(), &body)
            .unwrap();

        let request = server.receive(&message).unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.scheme, "https");
        assert_eq!(request.server, "example.com");
        assert_eq!(request.endpoint, "/api");
        assert_eq!(request.headers, header);

        message = server
            .respond(OhttpResponse {
                status_code: 200,
                headers: header.clone(),
                payload: body.clone(),
            })
            .unwrap();

        let response = session.decapsulate(&message).unwrap();
        assert_eq!(response.status_code, 200);
        assert_eq!(response.headers, header);
        assert_eq!(response.payload, body);
    }
}

uniffi::include_scaffolding!("as_ohttp_client");
