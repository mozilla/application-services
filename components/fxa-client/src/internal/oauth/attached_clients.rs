/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use super::super::http_client::GetAttachedClientResponse as AttachedClient;
use super::super::{util, CachedResponse, FirefoxAccount};
use crate::{Error, Result};

// An attached clients response is considered fresh for `ATTACHED_CLIENTS_FRESHNESS_THRESHOLD` ms.
const ATTACHED_CLIENTS_FRESHNESS_THRESHOLD: u64 = 60_000; // 1 minute

impl FirefoxAccount {
    /// Fetches the list of attached clients connected to the current account.
    pub fn get_attached_clients(&mut self) -> Result<Vec<AttachedClient>> {
        if let Some(a) = &self.attached_clients_cache {
            if util::now() < a.cached_at + ATTACHED_CLIENTS_FRESHNESS_THRESHOLD {
                return Ok(a.response.clone());
            }
        }
        let session_token = self.get_session_token()?;
        let response = self
            .client
            .get_attached_clients(self.state.config(), &session_token)?;

        self.attached_clients_cache = Some(CachedResponse {
            response: response.clone(),
            cached_at: util::now(),
            etag: "".into(),
        });

        Ok(response)
    }
}

impl TryFrom<AttachedClient> for crate::AttachedClient {
    type Error = Error;
    fn try_from(c: AttachedClient) -> Result<Self> {
        Ok(crate::AttachedClient {
            client_id: c.client_id,
            device_id: c.device_id,
            device_type: c.device_type,
            is_current_session: c.is_current_session,
            name: c.name,
            created_time: c.created_time.map(TryInto::try_into).transpose()?,
            last_access_time: c.last_access_time.map(TryInto::try_into).transpose()?,
            scope: c.scope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::{config::Config, http_client::MockFxAClient};
    use mockall::predicate::always;
    use mockall::predicate::eq;
    use std::sync::Arc;
    use sync15::DeviceType;

    #[test]
    fn test_get_attached_clients() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.set_session_token("session");

        let mut client = MockFxAClient::new();
        client
            .expect_get_attached_clients()
            .with(always(), eq("session"))
            .times(1)
            .returning(|_, _| {
                Ok(vec![AttachedClient {
                    client_id: Some("12345678".into()),
                    session_token_id: None,
                    refresh_token_id: None,
                    device_id: None,
                    device_type: DeviceType::Desktop,
                    is_current_session: true,
                    name: None,
                    created_time: None,
                    last_access_time: None,
                    scope: None,
                    user_agent: "attachedClientsUserAgent".into(),
                    os: None,
                }])
            });

        fxa.set_client(Arc::new(client));
        assert!(fxa.attached_clients_cache.is_none());

        let res = fxa.get_attached_clients();

        assert!(res.is_ok());
        assert!(fxa.attached_clients_cache.is_some());

        let cached_attached_clients_res = fxa.attached_clients_cache.unwrap();
        assert!(!cached_attached_clients_res.response.is_empty());
        assert!(cached_attached_clients_res.cached_at > 0);

        let cached_attached_clients = &cached_attached_clients_res.response[0];
        assert_eq!(
            cached_attached_clients.clone().client_id.unwrap(),
            "12345678".to_string()
        );
    }

    #[test]
    fn test_get_attached_clients_network_errors() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.set_session_token("session");

        let mut client = MockFxAClient::new();
        client
            .expect_get_attached_clients()
            .with(always(), eq("session"))
            .times(1)
            .returning(|_, _| {
                Err(Error::RemoteError {
                    code: 500,
                    errno: 101,
                    error: "Did not work!".to_owned(),
                    message: "Did not work!".to_owned(),
                    info: "Did not work!".to_owned(),
                })
            });

        fxa.set_client(Arc::new(client));
        assert!(fxa.attached_clients_cache.is_none());

        let res = fxa.get_attached_clients();
        assert!(res.is_err());
        assert!(fxa.attached_clients_cache.is_none());
    }
}
