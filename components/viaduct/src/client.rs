/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    header_names::USER_AGENT,
    new_backend::get_backend,
    settings::{validate_request, GLOBAL_SETTINGS},
    Request, Response, Result,
};

/// HTTP Client
///
/// This represents the "new" API.
/// See `README.md` for details about the transition from the old to new API.
#[derive(Default)]
pub struct Client {
    settings: ClientSettings,
}

impl Client {
    pub fn new(mut settings: ClientSettings) -> Self {
        settings.update_from_global_settings();
        Self { settings }
    }

    /// Create a client that uses OHTTP with the specified channel for all requests
    #[cfg(feature = "ohttp")]
    pub fn with_ohttp_channel(
        channel: &str,
        settings: ClientSettings,
    ) -> Result<Self, crate::ViaductError> {
        if !crate::ohttp::is_ohttp_channel_configured(channel) {
            return Err(crate::ViaductError::OhttpChannelNotConfigured(
                channel.to_string(),
            ));
        }
        let mut client_settings = settings;
        client_settings.ohttp_channel = Some(channel.to_string());
        Ok(Self::new(client_settings))
    }

    fn set_user_agent(&self, request: &mut Request) -> Result<()> {
        if let Some(user_agent) = &self.settings.user_agent {
            request.headers.insert_if_missing(USER_AGENT, user_agent)?;
        }
        Ok(())
    }

    pub async fn send(&self, mut request: Request) -> Result<Response> {
        validate_request(&request)?;
        self.set_user_agent(&mut request)?;

        // Check if this client should use OHTTP for all requests
        #[cfg(feature = "ohttp")]
        if let Some(channel) = &self.settings.ohttp_channel {
            crate::debug!(
                "Client configured for OHTTP channel '{}', processing request via OHTTP",
                channel
            );
            return crate::ohttp::process_ohttp_request(request, channel, self.settings.clone())
                .await;
        }

        // For non-OHTTP requests, use the normal backend
        crate::debug!("Processing request via standard backend");
        get_backend()?
            .send_request(request, self.settings.clone())
            .await
    }

    pub fn send_sync(&self, request: Request) -> Result<Response> {
        pollster::block_on(self.send(request))
    }
}

#[derive(Debug, uniffi::Record, Clone)]
#[repr(C)]
pub struct ClientSettings {
    /// Timeout for the entire request in ms (0 indicates no timeout).
    #[uniffi(default = 0)]
    pub timeout: u32,
    /// Maximum amount of redirects to follow (0 means redirects are not allowed)
    #[uniffi(default = 10)]
    pub redirect_limit: u32,
    /// OHTTP channel to use for all requests (if any)
    #[cfg(feature = "ohttp")]
    pub ohttp_channel: Option<String>,
    /// Client default user-agent.
    ///
    /// This overrides the global default user-agent and is used when no `User-agent` header is set
    /// directly in the Request.
    #[uniffi(default = None)]
    pub user_agent: Option<String>,
}

impl ClientSettings {
    pub fn update_from_global_settings(&mut self) {
        let settings = GLOBAL_SETTINGS.read();
        if self.user_agent.is_none() {
            self.user_agent = settings.default_user_agent.clone();
        }
    }
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "ios")]
            timeout: 7000,
            #[cfg(not(target_os = "ios"))]
            timeout: 10000,
            redirect_limit: 10,
            user_agent: None,
            #[cfg(feature = "ohttp")]
            ohttp_channel: None,
        }
    }
}

#[cfg(test)]
mod test {
    use url::Url;

    use super::*;
    use crate::settings;

    #[test]
    fn test_user_agent() {
        let mut req = Request::get(Url::parse("http://example.com/").unwrap());
        // No default user agent
        let client = Client::new(ClientSettings::default());
        client.set_user_agent(&mut req).unwrap();
        assert_eq!(req.headers.get(USER_AGENT), None);
        // Global user-agent set
        settings::set_global_default_user_agent("global-user-agent".into());
        let client = Client::new(ClientSettings::default());
        let mut req = Request::get(Url::parse("http://example.com/").unwrap());
        client.set_user_agent(&mut req).unwrap();
        assert_eq!(req.headers.get(USER_AGENT), Some("global-user-agent"));
        // ClientSettings overrides that
        let client = Client::new(ClientSettings {
            user_agent: Some("client-settings-user-agent".into()),
            ..ClientSettings::default()
        });
        let mut req = Request::get(Url::parse("http://example.com/").unwrap());
        client.set_user_agent(&mut req).unwrap();
        assert_eq!(
            req.headers.get(USER_AGENT),
            Some("client-settings-user-agent")
        );
        // Request header overrides that
        let mut req = Request::get(Url::parse("http://example.com/").unwrap());
        req.headers
            .insert(USER_AGENT, "request-user-agent")
            .unwrap();
        client.set_user_agent(&mut req).unwrap();
        assert_eq!(req.headers.get(USER_AGENT), Some("request-user-agent"));
    }
}
