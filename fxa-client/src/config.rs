/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::errors::*;
use reqwest;
use url::Url;

#[derive(Deserialize)]
struct ClientConfigurationResponse {
    auth_server_base_url: String,
    oauth_server_base_url: String,
    profile_server_base_url: String,
    sync_tokenserver_base_url: String,
}

#[derive(Deserialize)]
struct OpenIdConfigurationResponse {
    authorization_endpoint: String,
    issuer: String,
    jwks_uri: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    content_url: String,
    auth_url: String,
    oauth_url: String,
    profile_url: String,
    token_server_endpoint_url: String,
    authorization_endpoint: String,
    issuer: String,
    jwks_uri: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

impl Config {
    pub fn release() -> Result<Self> {
        Self::import_from("https://accounts.firefox.com")
    }

    pub fn stable_dev() -> Result<Self> {
        Self::import_from("https://stable.dev.lcip.org")
    }

    pub fn stage_dev() -> Result<Self> {
        Self::import_from("https://accounts.stage.mozaws.net")
    }

    pub(crate) fn new(
        content_url: String,
        auth_url: String,
        oauth_url: String,
        profile_url: String,
        token_server_endpoint_url: String,
        authorization_endpoint: String,
        issuer: String,
        jwks_uri: String,
        token_endpoint: String,
        userinfo_endpoint: String
    ) -> Self {
        Self {
            content_url,
            auth_url,
            oauth_url,
            profile_url,
            token_server_endpoint_url,
            authorization_endpoint,
            issuer,
            jwks_uri,
            token_endpoint,
            userinfo_endpoint
        }
    }

    pub fn import_from(content_url: &str) -> Result<Self> {
        let config_url = Url::parse(content_url)?.join(".well-known/fxa-client-configuration")?;
        let resp: ClientConfigurationResponse = reqwest::get(config_url)?.json()?;

        let openid_config_url = Url::parse(content_url)?.join(".well-known/openid-configuration")?;
        let openid_resp: OpenIdConfigurationResponse = reqwest::get(openid_config_url)?.json()?;

        Ok(Self {
            content_url: content_url.to_string(),
            auth_url: format!("{}/", resp.auth_server_base_url),
            oauth_url: format!("{}/", resp.oauth_server_base_url),
            profile_url: format!("{}/", resp.profile_server_base_url),
            token_server_endpoint_url: format!("{}/1.0/sync/1.5", resp.sync_tokenserver_base_url),
            authorization_endpoint: openid_resp.authorization_endpoint,
            issuer: openid_resp.issuer,
            jwks_uri: openid_resp.jwks_uri,
            token_endpoint: openid_resp.token_endpoint,
            userinfo_endpoint: openid_resp.userinfo_endpoint,
        })
    }

    pub fn content_url(&self) -> Result<Url> {
        Url::parse(&self.content_url).map_err(|e| e.into())
    }

    pub fn content_url_path(&self, path: &str) -> Result<Url> {
        self.content_url()?.join(path).map_err(|e| e.into())
    }

    pub fn auth_url(&self) -> Result<Url> {
        Url::parse(&self.auth_url).map_err(|e| e.into())
    }

    pub fn auth_url_path(&self, path: &str) -> Result<Url> {
        self.auth_url()?.join(path).map_err(|e| e.into())
    }

    pub fn profile_url(&self) -> Result<Url> {
        Url::parse(&self.profile_url).map_err(|e| e.into())
    }

    pub fn profile_url_path(&self, path: &str) -> Result<Url> {
        self.profile_url()?.join(path).map_err(|e| e.into())
    }

    pub fn oauth_url(&self) -> Result<Url> {
        Url::parse(&self.oauth_url).map_err(|e| e.into())
    }

    pub fn oauth_url_path(&self, path: &str) -> Result<Url> {
        self.oauth_url()?.join(path).map_err(|e| e.into())
    }

    pub fn token_server_endpoint_url(&self) -> Result<Url> {
        Url::parse(&self.token_server_endpoint_url).map_err(|e| e.into())
    }

    pub fn authorization_endpoint(&self) -> Result<Url> {
        Url::parse(&self.authorization_endpoint).map_err(|e| e.into())
    }

    pub fn issuer(&self) -> Result<Url> {
        Url::parse(&self.issuer).map_err(|e| e.into())
    }

    pub fn jwks_uri(&self) -> Result<Url> {
        Url::parse(&self.jwks_uri).map_err(|e| e.into())
    }

    pub fn token_endpoint(&self) -> Result<Url> {
        Url::parse(&self.token_endpoint).map_err(|e| e.into())
    }

    pub fn userinfo_endpoint(&self) -> Result<Url> {
        Url::parse(&self.userinfo_endpoint).map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths() {
        let config = Config {
            content_url: "https://stable.dev.lcip.org/".to_string(),
            auth_url: "https://stable.dev.lcip.org/auth/".to_string(),
            oauth_url: "https://oauth-stable.dev.lcip.org/".to_string(),
            profile_url: "https://stable.dev.lcip.org/profile/".to_string(),
            token_server_endpoint_url: "https://stable.dev.lcip.org/syncserver/token/1.0/sync/1.5"
                .to_string(),
            authorization_endpoint: "https://oauth-stable.dev.lcip.org/v1/authorization"
                .to_string(),
            issuer: "https://dev.lcip.org/".to_string(),
            jwks_uri: "https://oauth-stable.dev.lcip.org/v1/jwks".to_string(),
            token_endpoint: "https://oauth-stable.dev.lcip.org/v1/token".to_string(),
            userinfo_endpoint: "https://stable.dev.lcip.org/profile/v1/profile".to_string(),
        };
        assert_eq!(
            config.auth_url_path("v1/account/keys").unwrap().to_string(),
            "https://stable.dev.lcip.org/auth/v1/account/keys"
        );
        assert_eq!(
            config.oauth_url_path("v1/token").unwrap().to_string(),
            "https://oauth-stable.dev.lcip.org/v1/token"
        );
        assert_eq!(
            config.profile_url_path("v1/profile").unwrap().to_string(),
            "https://stable.dev.lcip.org/profile/v1/profile"
        );
        assert_eq!(
            config.content_url_path("oauth/signin").unwrap().to_string(),
            "https://stable.dev.lcip.org/oauth/signin"
        );
        assert_eq!(
            config.token_server_endpoint_url().unwrap().to_string(),
            "https://stable.dev.lcip.org/syncserver/token/1.0/sync/1.5"
        );
    }
}
