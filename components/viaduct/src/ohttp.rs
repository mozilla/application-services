/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use url::Url;

use crate::{Headers, Method, Request, Response, Result, ViaductError};

/// Configuration for an OHTTP channel
#[derive(Debug, Clone, uniffi::Record)]
pub struct OhttpConfig {
    /// The relay URL that will proxy requests
    pub relay_url: String,
    /// The target host that the relay will forward requests to
    pub target_host: String,
}

/// Cached relay configuration with expiration
#[derive(Debug, Clone)]
struct CachedRelayConfig {
    config_data: Vec<u8>,
    expires_at: SystemTime,
}

/// Global registry of OHTTP channel configurations
static OHTTP_CHANNELS: Lazy<RwLock<HashMap<String, OhttpConfig>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Cache for relay configurations
static CONFIG_CACHE: Lazy<RwLock<HashMap<String, CachedRelayConfig>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Configure an OHTTP channel with the given configuration
#[uniffi::export]
pub fn configure_ohttp_channel(channel: String, config: OhttpConfig) -> Result<()> {
    crate::info!(
        "Configuring OHTTP channel '{}' with relay: {}",
        channel,
        config.relay_url
    );

    // Validate URLs
    Url::parse(&config.relay_url)?;

    OHTTP_CHANNELS.write().insert(channel, config);
    Ok(())
}

/// Clear all OHTTP channel configurations
#[uniffi::export]
pub fn clear_ohttp_channels() {
    crate::info!("Clearing all OHTTP channel configurations");
    OHTTP_CHANNELS.write().clear();
    CONFIG_CACHE.write().clear();
}

/// Get the configuration for a specific OHTTP channel
pub fn get_ohttp_config(channel: &str) -> Result<OhttpConfig> {
    let channels = OHTTP_CHANNELS.read();
    channels
        .get(channel)
        .cloned()
        .ok_or_else(|| ViaductError::OhttpChannelNotConfigured(channel.to_string()))
}

/// Check if an OHTTP channel is configured
pub fn is_ohttp_channel_configured(channel: &str) -> bool {
    OHTTP_CHANNELS.read().contains_key(channel)
}

/// List all configured OHTTP channels
#[uniffi::export]
pub fn list_ohttp_channels() -> Vec<String> {
    OHTTP_CHANNELS.read().keys().cloned().collect()
}

/// Fetch and cache relay configuration
pub async fn fetch_relay_config(relay_url: &str) -> Result<Vec<u8>> {
    // Check cache first
    {
        let cache = CONFIG_CACHE.read();
        if let Some(cached) = cache.get(relay_url) {
            if cached.expires_at > SystemTime::now() {
                crate::trace!("Using cached config for relay: {}", relay_url);
                return Ok(cached.config_data.clone());
            }
        }
    }

    crate::info!("Fetching fresh config for relay: {}", relay_url);

    // Fetch fresh config from relay
    let config_url = format!("{}/ohttp-configs", relay_url.trim_end_matches('/'));
    let request = Request::get(Url::parse(&config_url)?);

    // Use the new backend to make the request (without OHTTP to avoid recursion)
    let backend = crate::new_backend::get_backend()?;
    let settings = crate::ClientSettings {
        timeout: 10000, // 10 second timeout for config fetching
        redirect_limit: 5,
    };

    let response = backend.send_request(request, settings).await?;

    if !response.is_success() {
        return Err(ViaductError::OhttpConfigFetchFailed(format!(
            "Failed to fetch config from {}: HTTP {}",
            config_url, response.status
        )));
    }

    let config_data = response.body;
    if config_data.is_empty() {
        return Err(ViaductError::OhttpConfigFetchFailed(
            "Empty config received from relay".to_string(),
        ));
    }

    // Cache the config for 1 hour
    let cached_config = CachedRelayConfig {
        config_data: config_data.clone(),
        expires_at: SystemTime::now() + Duration::from_secs(3600),
    };

    {
        let mut cache = CONFIG_CACHE.write();
        cache.insert(relay_url.to_string(), cached_config);
    }

    Ok(config_data)
}

/// Convert viaduct Headers to HashMap for as-ohttp-client
fn headers_to_hashmap(headers: &Headers) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for header in headers.iter() {
        map.insert(header.name.to_string(), header.value.to_string());
    }
    map
}

/// Convert HashMap back to viaduct Headers
fn hashmap_to_headers(map: HashMap<String, String>) -> Result<Headers> {
    let mut headers = Headers::new();
    for (name, value) in map {
        headers.insert(name, value)?;
    }
    Ok(headers)
}

/// Process an OHTTP request using the as-ohttp-client component
pub async fn process_ohttp_request(
    request: Request,
    channel: &str,
    settings: crate::ClientSettings,
) -> Result<Response> {
    crate::info!("Starting OHTTP request processing for channel: {}", channel);

    let config = get_ohttp_config(channel)?;
    crate::info!(
        "Got OHTTP config - relay: {}, target: {}",
        config.relay_url,
        config.target_host
    );

    // Fetch relay config
    crate::info!("Fetching relay config from: {}", config.relay_url);
    let relay_config_data = fetch_relay_config(&config.relay_url).await?;
    crate::info!("Fetched relay config: {} bytes", relay_config_data.len());

    // Create OHTTP session using as-ohttp-client
    crate::info!("Creating OHTTP session...");
    let ohttp_session = as_ohttp_client::OhttpSession::new(&relay_config_data).map_err(|e| {
        ViaductError::OhttpRequestError(format!("Failed to create OHTTP session: {}", e))
    })?;
    crate::info!("OHTTP session created successfully");

    // Prepare request components for as-ohttp-client
    let method = request.method.as_str();
    let scheme = request.url.scheme();
    let authority = request.url.host_str().unwrap_or("");
    let path_and_query = {
        let mut path = request.url.path().to_string();
        if let Some(query) = request.url.query() {
            path.push('?');
            path.push_str(query);
        }
        path
    };
    let headers_map = headers_to_hashmap(&request.headers);
    let payload = request.body.unwrap_or_default();

    crate::info!(
        "Request details: {} {} {}://{}{}",
        method,
        path_and_query,
        scheme,
        authority,
        path_and_query
    );
    crate::info!("Payload size: {} bytes", payload.len());

    // Encapsulate the request using as-ohttp-client
    crate::info!("Encapsulating request...");
    let encrypted_request = ohttp_session
        .encapsulate(
            method,
            scheme,
            authority,
            &path_and_query,
            headers_map,
            &payload,
        )
        .map_err(|e| {
            ViaductError::OhttpRequestError(format!("Failed to encapsulate request: {}", e))
        })?;
    crate::info!(
        "Request encapsulated: {} bytes encrypted",
        encrypted_request.len()
    );

    // Create HTTP request to send to the relay
    let relay_url = Url::parse(&config.relay_url)?;
    crate::info!("Sending to relay: {}", relay_url);

    let mut relay_headers = Headers::new();
    relay_headers.insert("Content-Type", "message/ohttp-req")?;

    let relay_request = Request {
        method: Method::Post,
        url: relay_url,
        headers: relay_headers,
        body: Some(encrypted_request),
        ohttp_channel: None, // Ensure no recursive OHTTP
    };

    // Send the encrypted request to the relay using the backend
    crate::info!("Sending request to relay...");
    let backend = crate::new_backend::get_backend()?;
    let relay_response = backend.send_request(relay_request, settings).await?;
    crate::info!("Relay responded with status: {}", relay_response.status);

    // Check if the relay responded successfully
    if !relay_response.is_success() {
        return Err(ViaductError::OhttpRequestError(format!(
            "OHTTP relay returned error: HTTP {} - {}",
            relay_response.status,
            String::from_utf8_lossy(&relay_response.body)
        )));
    }

    // Verify the response content type
    if let Some(content_type) = relay_response.headers.get("content-type") {
        if content_type != "message/ohttp-res" {
            crate::warn!(
                "OHTTP relay returned unexpected content-type: {}",
                content_type
            );
        }
    }

    // Decapsulate the encrypted response using as-ohttp-client
    crate::info!(
        "Decapsulating response: {} bytes",
        relay_response.body.len()
    );
    let ohttp_response = ohttp_session
        .decapsulate(&relay_response.body)
        .map_err(|e| {
            ViaductError::OhttpResponseError(format!("Failed to decapsulate OHTTP response: {}", e))
        })?;

    // Convert the as-ohttp-client response back to a viaduct Response
    let final_headers = hashmap_to_headers(ohttp_response.headers().clone())?;

    let final_response = Response {
        request_method: request.method,
        url: request.url,
        status: ohttp_response.status_code(),
        headers: final_headers,
        body: ohttp_response.payload().to_vec(),
    };

    crate::info!(
        "OHTTP request completed successfully for channel '{}': {} {}",
        channel,
        final_response.status,
        final_response.url
    );

    Ok(final_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_configuration() {
        clear_ohttp_channels();

        let config = OhttpConfig {
            relay_url: "https://relay.example.com".to_string(),
            target_host: "target.example.com".to_string(),
        };

        configure_ohttp_channel("test".to_string(), config.clone()).unwrap();

        assert!(is_ohttp_channel_configured("test"));
        assert!(!is_ohttp_channel_configured("nonexistent"));

        let retrieved = get_ohttp_config("test").unwrap();
        assert_eq!(retrieved.relay_url, config.relay_url);
        assert_eq!(retrieved.target_host, config.target_host);

        let channels = list_ohttp_channels();
        assert_eq!(channels, vec!["test"]);

        clear_ohttp_channels();
        assert!(!is_ohttp_channel_configured("test"));
    }

    #[test]
    fn test_headers_conversion() {
        let mut headers = Headers::new();
        headers.insert("Content-Type", "application/json").unwrap();
        headers.insert("Authorization", "Bearer token").unwrap();

        let map = headers_to_hashmap(&headers);

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("content-type").unwrap(), "application/json");
        assert_eq!(map.get("authorization").unwrap(), "Bearer token");

        let headers_back = hashmap_to_headers(map).unwrap();

        assert_eq!(
            headers_back.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(headers_back.get("Authorization").unwrap(), "Bearer token");
    }

    #[test]
    fn test_ohttp_session_creation() {
        // Test with a simple config (this would normally come from a real relay)
        let test_server = as_ohttp_client::OhttpTestServer::new();
        let config = test_server.get_config();

        let session_result = as_ohttp_client::OhttpSession::new(&config);
        assert!(session_result.is_ok());
    }
}
