/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use hawk;

use reqwest::{Client, Request, Url};
use hyper::header::{Authorization, Bearer};
use error::{self, Result};
use std::fmt;
use std::borrow::{Borrow, Cow};

/// Tokenserver's timestamp is X-Timestamp and not X-Weave-Timestamp.
header! { (RetryAfter, "Retry-After") => [f64] }

/// Tokenserver's timestamp is X-Timestamp and not X-Weave-Timestamp. The value is in seconds.
header! { (XTimestamp, "X-Timestamp") => [f64] }

/// OAuth tokenserver api uses this instead of X-Client-State.
header! { (XKeyID, "X-KeyID") => [String] }

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TokenserverToken {
    pub id: String,
    pub key: String,
    pub api_endpoint: String,
    pub uid: u64,
    pub duration: u64,
    // This is treated as optional by at least the desktop client,
    // but AFAICT it's always present.
    pub hashed_fxa_uid: String,
}

/// This is really more of a TokenAuthenticator.
pub struct TokenserverClient {
    token: TokenserverToken,
    server_timestamp: f64,
    credentials: hawk::Credentials,
}

// hawk::Credentials doesn't implement debug -_-
impl fmt::Debug for TokenserverClient {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        f.debug_struct("TokenserverClient")
         .field("token", &self.token)
         .field("server_timestamp", &self.server_timestamp)
         .field("credentials", &"(omitted)")
         .finish()
    }
}

fn token_url(base_url: &str) -> Result<Url> {
    let mut url = Url::parse(base_url)?;
    // kind of gross but avoids problems if base_url has a trailing slash.
    url.path_segments_mut()
        // We can't do anything anyway if this is the case.
       .map_err(|_| error::unexpected("Bad tokenserver url (cannot be base)"))?
       .extend(&["1.0", "sync", "1.5"]);
    Ok(url)
}

impl TokenserverClient {
    #[inline]
    pub fn server_timestamp(&self) -> f64 {
        self.server_timestamp
    }

    #[inline]
    pub fn token(&self) -> &TokenserverToken {
        &self.token
    }

    pub fn new(request_client: &Client, base_url: &str, access_token: String, key_id: String) -> Result<TokenserverClient> {
        let mut resp = request_client.get(token_url(base_url)?)
                                     .header(Authorization(Bearer { token: access_token }))
                                     .header(XKeyID(key_id))
                                     .send()?;

        if !resp.status().is_success() {
            warn!("Non-success status when fetching token: {}", resp.status());
            // TODO: the body should be JSON and contain a status parameter we might need?
            debug!("  Response body {}", resp.text().unwrap_or("???".into()));
            if let Some(seconds) = resp.headers().get::<RetryAfter>().map(|h| **h) {
                bail!(error::ErrorKind::BackoffError(seconds));
            }
            bail!(error::ErrorKind::TokenserverHttpError(resp.status()));
        }

        let mut token: TokenserverToken = resp.json()?;
        // Add a trailing slash to the api endpoint instead of at each endpoint. This is required
        // for the uid not to get dropped by rust's url crate (which wants stuff like
        // `Url::parse("http://example.com/foo.html").join("style.css")` to resolve to
        // `http://example.com/style.css`, annoyingly.
        token.api_endpoint.push('/');

        let timestamp = resp.headers()
                            .get::<XTimestamp>()
                            .map(|h| **h)
                            .ok_or_else(|| error::unexpected(
                                "Missing or corrupted X-Timestamp header from token server"))?;
        let credentials = hawk::Credentials {
            id: token.id.clone(),
            key: hawk::Key::new(token.key.as_bytes(), &hawk::SHA256),
        };
        Ok(TokenserverClient { token, credentials, server_timestamp: timestamp })
    }

    pub fn authorization(&self, req: &Request) -> Result<Authorization<String>> {
        let url = req.url();

        let path_and_query = match url.query() {
            None => Cow::from(url.path()),
            Some(qs) => Cow::from(format!("{}?{}", url.path(), qs))
        };

        let host = url.host_str().ok_or_else(||
            error::unexpected("Tried to authorize bad URL using hawk (no host)"))?;

        // Known defaults exist for https? (among others), so this should be impossible
        let port = url.port_or_known_default().ok_or_else(||
            error::unexpected(
                "Tried to authorize bad URL using hawk (no port -- unknown protocol?)"))?;

        let header = hawk::RequestBuilder::new(
            req.method().as_ref(),
            host,
            port,
            path_and_query.borrow()
        ).request().make_header(&self.credentials)?;

        Ok(Authorization(format!("Hawk {}", header)))
    }
}
