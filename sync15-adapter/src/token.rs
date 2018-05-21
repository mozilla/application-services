/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use hawk;

use reqwest::{Client, Request, Url};
use hyper::header::{Authorization, Bearer};
use error::{self, Result};
use std::fmt;
use std::borrow::{Borrow, Cow};
use std::time::{SystemTime, Duration};
use std::cell::{RefCell};
use std::rc::Rc;
use util::ServerTimestamp;

/// Tokenserver's timestamp is X-Timestamp and not X-Weave-Timestamp.
header! { (RetryAfter, "Retry-After") => [f64] }

/// Tokenserver's timestamp is X-Timestamp and not X-Weave-Timestamp. The value is in seconds.
header! { (XTimestamp, "X-Timestamp") => [ServerTimestamp] }

/// OAuth tokenserver api uses this instead of X-Client-State.
header! { (XKeyID, "X-KeyID") => [String] }

fn token_url(base_url: &str) -> Result<Url> {
    let mut url = Url::parse(base_url)?;
    // kind of gross but avoids problems if base_url has a trailing slash.
    url.path_segments_mut()
        // We can't do anything anyway if this is the case.
       .map_err(|_| error::unexpected("Bad tokenserver url (cannot be base)"))?
       .extend(&["1.0", "sync", "1.5"]);
    Ok(url)
}

// The TokenserverToken is the token as received directly from the token server
// and deserialized from JSON.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
struct TokenserverToken {
    id: String,
    key: String,
    api_endpoint: String,
    uid: u64,
    duration: u64,
    hashed_fxa_uid: String,
}

// A context stored by our TokenserverClient when it has a TokenState::Token
// state.
struct TokenContext {
    token: TokenserverToken,
    credentials: hawk::Credentials,
    server_timestamp: ServerTimestamp,
    valid_until: SystemTime,
}

// hawk::Credentials doesn't implement debug -_-
impl fmt::Debug for TokenContext {
    fn fmt(&self, f: &mut fmt::Formatter) -> ::std::result::Result<(), fmt::Error> {
        f.debug_struct("TokenContext")
         .field("token", &self.token)
         .field("credentials", &"(omitted)")
         .field("server_timestamp", &self.server_timestamp)
         .field("valid_until", &self.valid_until)
         .finish()
    }
}

impl TokenContext {
    fn new(tsc: &TokenserverClient, request_client: &Client) -> Result<TokenContext> {
        let mut resp = request_client.get(token_url(&tsc.base_url)?)
                                     .header(Authorization(Bearer { token: tsc.access_token.clone() }))
                                     .header(XKeyID(tsc.key_id.clone()))
                                     .send()?;

        if !resp.status().is_success() {
            warn!("Non-success status when fetching token: {}", resp.status());
            // TODO: the body should be JSON and contain a status parameter we might need?
            debug!("  Response body {}", resp.text().unwrap_or("???".into()));
            // XXX - shouldn't we "chain" these errors - ie, a BackoffError could
            // have a TokenserverHttpError as its cause?
            if let Some(ms) = resp.headers().get::<RetryAfter>().map(|h| (**h * 1000f64) as u64) {
                let when = SystemTime::now() + Duration::from_millis(ms);
                bail!(error::ErrorKind::BackoffError(when));
            }
            bail!(error::ErrorKind::TokenserverHttpError(resp.status()));
        }

        let token: TokenserverToken = resp.json()?;
        let valid_until = SystemTime::now() + Duration::from_secs(token.duration);

        let timestamp = resp.headers()
                            .get::<XTimestamp>()
                            .map(|h| **h)
                            .ok_or_else(|| error::unexpected(
                                "Missing or corrupted X-Timestamp header from token server"))?;

        let credentials = hawk::Credentials {
            id: token.id.clone(),
            key: hawk::Key::new(token.key.as_bytes(), hawk::Digest::sha256())?,
        };

        Ok(TokenContext {
            token,
            credentials,
            server_timestamp: timestamp,
            valid_until,
        })
    }

    fn is_valid(&self) -> bool {
        // We could consider making the duration a little shorter - if it
        // only has 1 second validity there seems a reasonable chance it will
        // have expired by the time it gets presented to the remote that wants
        // it.
        // Either way though, we will eventually need to handle a token being
        // rejected as a non-fatal error and recover, so maybe we don't care?
        SystemTime::now() < self.valid_until
    }

    fn authorization(&self, req: &Request) -> Result<Authorization<String>> {
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

// The state our TokenserverClient holds to reflect the state of the token.
#[derive(Debug)]
enum TokenState {
    // We've never fetched a token.
    NoToken,
    // Have a token and last we checked it remained valid.
    Token(TokenContext),
    // We failed to fetch a token. First elt is the error, second elt is
    // the api_endpoint we had before we failed to fetch a new token (or
    // None if the very first attempt at fetching a token failed)
    Failed(Option<error::Error>, Option<String>),
    // Previously failed and told to back-off for SystemTime duration. Second
    // elt is the api_endpoint we had before we hit the backoff error.
    // XXX - should we roll Backoff and Failed together?
    Backoff(SystemTime, Option<String>),
    // api_endpoint changed - we are never going to get a token nor move out
    // of this state.
    NodeReassigned,
}

/// The TokenserverClient - long lived and fetches tokens on demand (eg, when
/// first needed, or when an existing one expires.)
#[derive(Debug)]
pub struct TokenserverClient {
    request_client: Rc<Client>,
    // The stuff needed to fetch a token.
    base_url: String,
    access_token: String,
    key_id: String,
    // Our token state (ie, whether we have a token, and if not, why not)
    current_state: RefCell<TokenState>,
}

impl TokenserverClient {
    pub fn new(request_client: Rc<Client>, base_url: String, access_token: String, key_id: String) -> TokenserverClient {
        TokenserverClient {
            request_client,
            base_url,
            access_token,
            key_id,
            current_state: RefCell::new(TokenState::NoToken),
        }
    }

    // Attempt to fetch a new token and return a new state reflecting that
    // operation. If it worked a TokenState::Token state will be returned, but
    // errors may cause other states.
    fn fetch_token(&self, previous_endpoint: Option<&str>) -> TokenState {
        match TokenContext::new(self, &self.request_client) {
            Ok(tc) => {
                // We got a new token - check that the endpoint is the same
                // as a previous endpoint we saw (if any)
                match previous_endpoint {
                    Some(prev) => {
                        if prev == tc.token.api_endpoint {
                            TokenState::Token(tc)
                        } else {
                            warn!("api_endpoint changed from {} to {}", prev, tc.token.api_endpoint);
                            TokenState::NodeReassigned
                        }
                    },
                    None => {
                        // Never had an api_endpoint in the past, so this is OK.
                        TokenState::Token(tc)
                    }
                }
            },
            Err(e) => {
                match e {
                    error::Error(error::ErrorKind::BackoffError(ref be), _) => {
                        TokenState::Backoff(*be, previous_endpoint.map(|s| s.to_string()))
                    }
                    _ => {
                        TokenState::Failed(Some(e), previous_endpoint.map(|s| s.to_string()))
                    }
                }
            }
        }
    }

    // Given the state we are currently in, return a new current state.
    // Returns None if the current state should be used (eg, if we are
    // holding a token that remains valid) or Some() if the state has changed
    // (which may have changed to a state with a token or an error state)
    fn advance_state(&self, state: &TokenState) -> Option<TokenState> {
        match state {
            TokenState::NoToken => {
                Some(self.fetch_token(None))
            },
            TokenState::Failed(_, existing_endpoint) => {
                Some(self.fetch_token(existing_endpoint.as_ref().map(|e| e.as_str())))
            },
            TokenState::Token(existing_context) => {
                if existing_context.is_valid() {
                    None
                } else {
                    Some(self.fetch_token(Some(existing_context.token.api_endpoint.as_str())))
                }
            },
            TokenState::Backoff(ref until, ref existing_endpoint) => {
                if let Ok(remaining) = until.duration_since(SystemTime::now()) {
                    debug!("enforcing existing backoff - {:?} remains", remaining);
                    None
                } else {
                    // backoff period is over
                    Some(self.fetch_token(existing_endpoint.as_ref().map(|e| e.as_str())))
                }
            },
            TokenState::NodeReassigned => {
                // We never leave this state.
                None
            }
        }
    }

    fn with_token<T, F>(&self, func: F) -> Result<T>
            where F: FnOnce(&TokenContext) -> Result<T> {

        // first get a mutable ref to our existing state, advance to the
        // state we will use, then re-stash that state for next time.
        let state: &mut TokenState = &mut self.current_state.borrow_mut();
        match self.advance_state(state) {
            Some(new_state) => *state = new_state,
            None => ()
        }

        // Now re-fetch the state we should use for this call - if it's
        // anything other than TokenState::Token we will fail.
        match state {
            TokenState::NoToken => {
                // it should be impossible to get here.
                panic!("Can't be in NoToken state after advancing");
            }
            TokenState::Token(ref token_context) => {
                // make the call.
                func(token_context)
            }
            TokenState::Failed(e, _) => {
                // We swap the error out of the state enum and return it.
                return Err(e.take().unwrap());
            }
            TokenState::NodeReassigned => {
                // this is unrecoverable.
                bail!(error::ErrorKind::StorageResetError);
            }
            TokenState::Backoff(ref remaining, _) => {
                bail!(error::ErrorKind::BackoffError(*remaining));
            }
        }
    }

    pub fn authorization(&self, req: &Request) -> Result<Authorization<String>> {
        Ok(self.with_token(|ctx| ctx.authorization(req))?)
    }

    pub fn api_endpoint(&self) -> Result<String> {
        Ok(self.with_token(|ctx| Ok(ctx.token.api_endpoint.clone()))?)
    }
    // TODO: we probably want a "drop_token/context" type method so that when
    // using a token with some validity fails the caller can force a new one
    // (in which case the new token request will probably fail with a 401)
}

#[cfg(test)]
mod tests {
    use super::*;

    use reqwest::{Client};

    #[test]
    fn test_something() {
        let client = Rc::new(Client::builder().timeout(Duration::from_secs(30)).build().expect("can't build client"));
        let tsc = TokenserverClient::new(client.clone(),
                                         String::from("base_url"),
                                         String::from("access_token"),
                                         String::from("key_id"));

        // TODO: make this actually useful!
        let _e = tsc.api_endpoint().expect_err("should fail");
        println!("FAILED WITH {}", _e.kind());
        // XXX - this will fail with |ErrorKind::BadUrl(RelativeUrlWithoutBase)|
        // but I'm not sure how to test it!
        //assert_eq!(_e, error::ErrorKind::BadUrl(RelativeUrlWithoutBase));
    }
}
