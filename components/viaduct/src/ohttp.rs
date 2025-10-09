/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::ohttp_client::OhttpSession;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use url::Url;

use crate::{Headers, Method, Request, Response, Result, ViaductError};

/// Configuration for an OHTTP channel
#[derive(Debug, Clone, uniffi::Record)]
pub struct OhttpConfig {
    /// The relay URL that will proxy requests
    pub relay_url: String,
    /// The gateway host that provides encryption keys and decrypts requests
    pub gateway_host: String,
}

/// Cached gateway configuration with expiration
#[derive(Debug, Clone)]
struct CachedGatewayConfig {
    config_data: Vec<u8>,
    expires_at: SystemTime,
}

/// Global registry of OHTTP channel configurations
static OHTTP_CHANNELS: Lazy<RwLock<HashMap<String, OhttpConfig>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Cache for gateway configurations with async protection
static CONFIG_CACHE: Lazy<RwLock<HashMap<String, CachedGatewayConfig>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Configure an OHTTP channel with the given configuration
/// If an existing OHTTP config exists with the same name, it will be overwritten
#[uniffi::export]
pub fn configure_ohttp_channel(channel: String, config: OhttpConfig) -> Result<()> {
    crate::trace!(
        "Configuring OHTTP channel '{}' with relay: {}, gateway: {}",
        channel,
        config.relay_url,
        config.gateway_host
    );

    // Validate URLs
    let parsed_relay = Url::parse(&config.relay_url)?;
    crate::trace!(
        "Relay URL validated: scheme={}, host={:?}",
        parsed_relay.scheme(),
        parsed_relay.host_str()
    );

    // Validate gateway host format
    if config.gateway_host.is_empty() {
        return Err(crate::ViaductError::NetworkError(
            "Gateway host cannot be empty".to_string(),
        ));
    }
    crate::trace!("Gateway host validated: {}", config.gateway_host);

    OHTTP_CHANNELS.write().insert(channel.clone(), config);
    crate::trace!("OHTTP channel '{}' configured successfully", channel);
    Ok(())
}

/// Configure default OHTTP channels for common Mozilla services
/// This sets up:
/// - "relay1": For general telemetry and services through Mozilla's shared gateway
/// - "merino": For Firefox Suggest recommendations through Merino's dedicated relay/gateway
#[uniffi::export]
pub fn configure_default_ohttp_channels() -> Result<()> {
    crate::trace!("Configuring default OHTTP channels");

    // Configure relay1 for general purpose OHTTP
    // Fastly relay forwards to Mozilla's shared gateway
    configure_ohttp_channel(
        "relay1".to_string(),
        OhttpConfig {
            relay_url: "https://mozilla-ohttp.fastly-edge.com/".to_string(),
            gateway_host: "prod.ohttp-gateway.prod.webservices.mozgcp.net".to_string(),
        },
    )?;

    // Configure merino with its dedicated relay and integrated gateway
    configure_ohttp_channel(
        "merino".to_string(),
        OhttpConfig {
            relay_url: "https://ohttp-relay-merino-prod.edgecompute.app/".to_string(),
            gateway_host: "prod.merino.prod.webservices.mozgcp.net".to_string(),
        },
    )?;

    crate::trace!("Default OHTTP channels configured successfully");
    Ok(())
}

/// Clear all OHTTP channel configurations
#[uniffi::export]
pub fn clear_ohttp_channels() {
    crate::trace!("Clearing all OHTTP channel configurations");
    OHTTP_CHANNELS.write().clear();
    CONFIG_CACHE.write().clear();
}

/// Get the configuration for a specific OHTTP channel
pub fn get_ohttp_config(channel: &str) -> Result<OhttpConfig> {
    crate::trace!("Looking up OHTTP config for channel: {}", channel);
    let channels = OHTTP_CHANNELS.read();
    match channels.get(channel) {
        Some(config) => {
            crate::trace!(
                "Found OHTTP config for channel '{}': relay={}, gateway={}",
                channel,
                config.relay_url,
                config.gateway_host
            );
            Ok(config.clone())
        }
        None => {
            let available_channels: Vec<_> = channels.keys().collect();
            crate::error!(
                "OHTTP channel '{}' not configured. Available channels: {:?}",
                channel,
                available_channels
            );
            Err(ViaductError::OhttpChannelNotConfigured(channel.to_string()))
        }
    }
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

/// Fetch and cache gateway configuration (encryption keys)
pub async fn fetch_gateway_config(gateway_host: &str) -> Result<Vec<u8>> {
    if let Some(cached) = read_config_from_cache(gateway_host) {
        return Ok(cached);
    }

    // Could be that multiple threads fetch an already existing config
    // because we don't double check here. We are currently ok with that
    // to keep the code simpler
    let config_data = fetch_config_from_network(gateway_host).await?;

    // Update cache (last writer wins)
    {
        let mut cache = CONFIG_CACHE.write();
        cache.insert(
            gateway_host.to_string(),
            CachedGatewayConfig {
                config_data: config_data.clone(),
                // Set the cache expiry to 1 day
                expires_at: SystemTime::now() + Duration::from_secs(60 * 60 * 24),
            },
        );
    }

    Ok(config_data)
}

/// Read from cache if valid
fn read_config_from_cache(gateway_host: &str) -> Option<Vec<u8>> {
    let cache = CONFIG_CACHE.read();
    check_cache_entry(&cache, gateway_host)
}

/// Check if cache entry exists and is valid
fn check_cache_entry(
    cache: &HashMap<String, CachedGatewayConfig>,
    gateway_host: &str,
) -> Option<Vec<u8>> {
    cache.get(gateway_host).and_then(|cached| {
        if cached.expires_at > SystemTime::now() {
            crate::trace!("Using cached config for gateway: {}", gateway_host);
            Some(cached.config_data.clone())
        } else {
            crate::trace!("Cached config for {} has expired", gateway_host);
            None
        }
    })
}

/// Fetch config from network and update cache
async fn fetch_config_from_network(gateway_host: &str) -> Result<Vec<u8>> {
    let gateway_url = format!("https://{}", gateway_host);
    let config_url = Url::parse(&gateway_url)?.join("ohttp-configs")?;

    let request = Request::get(config_url.clone());
    let backend = crate::new_backend::get_backend()?;
    let settings = crate::ClientSettings {
        timeout: 10000,
        redirect_limit: 5,
        #[cfg(feature = "ohttp")]
        ohttp_channel: None,
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
            "Empty config received from gateway".to_string(),
        ));
    }

    crate::trace!("Successfully fetched {} bytes", config_data.len());
    Ok(config_data)
}

/// Process an OHTTP request using the OHTTP client component
pub async fn process_ohttp_request(
    request: Request,
    channel: &str,
    settings: crate::ClientSettings,
) -> Result<Response> {
    let overall_start = std::time::Instant::now();
    crate::trace!(
        "=== Starting OHTTP request processing for channel: '{}' ===",
        channel
    );
    crate::trace!("Target URL: {} {}", request.method, request.url);

    let config = get_ohttp_config(channel)?;
    crate::trace!(
        "Retrieved OHTTP config - relay: {}, gateway: {}",
        config.relay_url,
        config.gateway_host
    );

    // Fetch gateway config (encryption keys)
    crate::trace!(
        "Step 1: Fetching gateway encryption keys from: {}",
        config.gateway_host
    );
    let gateway_config_start = std::time::Instant::now();
    let gateway_config_data = fetch_gateway_config(&config.gateway_host).await?;
    let gateway_config_duration = gateway_config_start.elapsed();
    crate::trace!(
        "Gateway config fetched: {} bytes in {:?}",
        gateway_config_data.len(),
        gateway_config_duration
    );

    // Create OHTTP session using the gateway's encryption keys
    crate::trace!("Step 2: Creating OHTTP session with gateway keys...");
    let session_start = std::time::Instant::now();
    let ohttp_session = OhttpSession::new(&gateway_config_data).map_err(|e| {
        crate::error!("Failed to create OHTTP session: {}", e);
        ViaductError::OhttpRequestError(format!("Failed to create OHTTP session: {}", e))
    })?;
    let session_duration = session_start.elapsed();
    crate::trace!(
        "OHTTP session created successfully in {:?}",
        session_duration
    );

    // Prepare request components - these come from the actual request URL (target)
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
    let headers_map: HashMap<String, String> = request.headers.clone().into();
    let payload = request.body.unwrap_or_default();

    crate::trace!(
        "Step 3: Preparing request - {} {}://{}{}",
        method,
        scheme,
        authority,
        path_and_query
    );
    crate::trace!("Request headers: {} total", headers_map.len());
    crate::trace!("Request payload: {} bytes", payload.len());

    // Encapsulate the request using the OHTTP session
    crate::trace!("Step 4: Encapsulating request with OHTTP...");
    let encap_start = std::time::Instant::now();
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
            crate::error!("Failed to encapsulate request: {}", e);
            ViaductError::OhttpRequestError(format!("Failed to encapsulate request: {}", e))
        })?;
    let encap_duration = encap_start.elapsed();
    crate::trace!(
        "Request encapsulated: {} bytes → {} bytes encrypted in {:?}",
        payload.len(),
        encrypted_request.len(),
        encap_duration
    );

    // Create HTTP request to send to the relay
    let relay_url = Url::parse(&config.relay_url)?;
    crate::trace!("Step 5: Sending encrypted request to relay: {}", relay_url);

    let mut relay_headers = Headers::new();
    relay_headers.insert("Content-Type", "message/ohttp-req")?;

    let relay_request = Request {
        method: Method::Post,
        url: relay_url.clone(),
        headers: relay_headers,
        body: Some(encrypted_request),
    };

    // Send the encrypted request to the relay using the backend
    crate::trace!("Sending to relay with timeout: {}ms", settings.timeout);
    let relay_start = std::time::Instant::now();
    let backend = crate::new_backend::get_backend()?;
    let relay_response = backend.send_request(relay_request, settings).await?;
    let relay_duration = relay_start.elapsed();

    crate::trace!(
        "Relay responded: HTTP {} in {:?}",
        relay_response.status,
        relay_duration
    );

    // Check if the relay responded successfully
    if !relay_response.is_success() {
        crate::error!(
            "OHTTP relay {} returned error: HTTP {} - {}",
            relay_url,
            relay_response.status,
            String::from_utf8_lossy(&relay_response.body)
        );
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
                "OHTTP relay returned unexpected content-type: {} (expected: message/ohttp-res)",
                content_type
            );
        } else {
            crate::trace!("Relay response content-type verified: {}", content_type);
        }
    } else {
        crate::warn!("OHTTP relay response missing content-type header");
    }

    // Decapsulate the encrypted response using the OHTTP session
    crate::trace!(
        "Step 6: Decapsulating response ({} bytes from relay)...",
        relay_response.body.len()
    );
    let decap_start = std::time::Instant::now();
    let ohttp_response = ohttp_session
        .decapsulate(&relay_response.body)
        .map_err(|e| {
            crate::error!("Failed to decapsulate OHTTP response: {}", e);
            ViaductError::OhttpResponseError(format!("Failed to decapsulate OHTTP response: {}", e))
        })?;
    let decap_duration = decap_start.elapsed();

    // Convert the OHTTP response back to a viaduct Response
    let (status, headers_map, body) = ohttp_response.into_parts();
    let final_headers = Headers::try_from_hashmap(headers_map)?;

    let final_response = Response {
        request_method: request.method,
        url: request.url,
        status,
        headers: final_headers,
        body,
    };

    let overall_duration = overall_start.elapsed();
    crate::trace!(
        "=== OHTTP request completed successfully for channel '{}' ===",
        channel
    );
    crate::trace!(
        "Final result: HTTP {} with {} bytes (total time: {:?})",
        final_response.status,
        final_response.body.len(),
        overall_duration
    );
    crate::trace!(
        "Timing breakdown - Config: {:?}, Session: {:?}, Encap: {:?}, Relay: {:?}, Decap: {:?}",
        gateway_config_duration,
        session_duration,
        encap_duration,
        relay_duration,
        decap_duration
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
            gateway_host: "gateway.example.com".to_string(),
        };

        configure_ohttp_channel("test".to_string(), config.clone()).unwrap();

        assert!(is_ohttp_channel_configured("test"));
        assert!(!is_ohttp_channel_configured("nonexistent"));

        let retrieved = get_ohttp_config("test").unwrap();
        assert_eq!(retrieved.relay_url, config.relay_url);
        assert_eq!(retrieved.gateway_host, config.gateway_host);

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

        let map: HashMap<String, String> = headers.clone().into();

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("content-type").unwrap(), "application/json");
        assert_eq!(map.get("authorization").unwrap(), "Bearer token");

        let headers_back = Headers::try_from_hashmap(map).unwrap();

        assert_eq!(
            headers_back.get("Content-Type").unwrap(),
            "application/json"
        );
        assert_eq!(headers_back.get("Authorization").unwrap(), "Bearer token");
    }
}
