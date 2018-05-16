use super::errors::*;
use reqwest;
use url::Url;

#[derive(Deserialize)]
struct FxAClientConfigurationResponse {
    auth_server_base_url: String,
    oauth_server_base_url: String,
    profile_server_base_url: String,
    //sync_tokenserver_base_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    content_url: String,
    auth_url: String,
    oauth_url: String,
    profile_url: String,
}

impl Config {
    pub fn release() -> Result<Config> {
        Config::import_from("https://accounts.firefox.com")
    }

    pub fn stable() -> Result<Config> {
        Config::import_from("https://stable.dev.lcip.org")
    }

    pub fn import_from(content_url: &str) -> Result<Config> {
        let config_url = Url::parse(content_url)?.join(".well-known/fxa-client-configuration")?;
        let resp: FxAClientConfigurationResponse = reqwest::get(config_url)?.json()?;
        Ok(Config {
            content_url: content_url.to_string(),
            auth_url: format!("{}/", resp.auth_server_base_url),
            oauth_url: format!("{}/", resp.oauth_server_base_url),
            profile_url: format!("{}/", resp.profile_server_base_url),
        })
    }

    pub fn content_url(&self) -> Result<Url> {
        Ok(Url::parse(&self.content_url)?)
    }

    pub fn content_url_path(&self, path: &str) -> Result<Url> {
        Ok(self.content_url()?.join(path)?)
    }

    pub fn auth_url(&self) -> Result<Url> {
        Ok(Url::parse(&self.auth_url)?)
    }

    pub fn auth_url_path(&self, path: &str) -> Result<Url> {
        Ok(self.auth_url()?.join(path)?)
    }

    pub fn profile_url(&self) -> Result<Url> {
        Ok(Url::parse(&self.profile_url)?)
    }

    pub fn profile_url_path(&self, path: &str) -> Result<Url> {
        Ok(self.profile_url()?.join(path)?)
    }

    pub fn oauth_url(&self) -> Result<Url> {
        Ok(Url::parse(&self.oauth_url)?)
    }

    pub fn oauth_url_path(&self, path: &str) -> Result<Url> {
        Ok(self.oauth_url()?.join(path)?)
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
    }
}
