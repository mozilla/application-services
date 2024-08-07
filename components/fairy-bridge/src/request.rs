/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{headers, FairyBridgeError, Response, Result};
use pollster::FutureExt;
use std::borrow::Cow;
use std::collections::HashMap;
use url::Url;

// repr(C) so that it can be easily used with the C backend.
#[derive(uniffi::Enum)]
#[repr(C)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

#[derive(uniffi::Record)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

/// Http request
///
/// These are created using the builder pattern, then sent over the network using the `send()`
/// method.
impl Request {
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
        }
    }

    pub async fn send(self) -> crate::Result<Response> {
        let mut response = match crate::REGISTERED_BACKEND.get() {
            Some(backend) => backend.clone().send_request(self).await,
            None => Err(FairyBridgeError::NoBackendInitialized),
        }?;
        response.headers = response
            .headers
            .into_iter()
            .map(|(name, value)| Ok((headers::normalize_request_header(name)?, value)))
            .collect::<crate::Result<HashMap<_, _>>>()?;
        Ok(response)
    }

    pub fn send_sync(self) -> crate::Result<Response> {
        self.send().block_on()
    }

    /// Alias for `Request::new(Method::Get, url)`, for convenience.
    pub fn get(url: Url) -> Self {
        Self::new(Method::Get, url)
    }

    /// Alias for `Request::new(Method::Patch, url)`, for convenience.
    pub fn patch(url: Url) -> Self {
        Self::new(Method::Patch, url)
    }

    /// Alias for `Request::new(Method::Post, url)`, for convenience.
    pub fn post(url: Url) -> Self {
        Self::new(Method::Post, url)
    }

    /// Alias for `Request::new(Method::Put, url)`, for convenience.
    pub fn put(url: Url) -> Self {
        Self::new(Method::Put, url)
    }

    /// Alias for `Request::new(Method::Delete, url)`, for convenience.
    pub fn delete(url: Url) -> Self {
        Self::new(Method::Delete, url)
    }

    /// Add all the provided headers to the list of headers to send with this
    /// request.
    pub fn headers<'a, I, K, V>(mut self, to_add: I) -> crate::Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<Cow<'a, str>>,
        V: Into<String>,
    {
        for (name, value) in to_add {
            self = self.header(name, value)?
        }
        Ok(self)
    }

    /// Add the provided header to the list of headers to send with this request.
    ///
    /// This returns `Err` if `val` contains characters that may not appear in
    /// the body of a header.
    ///
    /// ## Example
    /// ```
    /// # use fairy_bridge::{Request, headers};
    /// # use url::Url;
    /// # fn main() -> fairy_bridge::Result<()> {
    /// # let some_url = url::Url::parse("https://www.example.com").unwrap();
    /// Request::post(some_url)
    ///     .header(headers::CONTENT_TYPE, "application/json")?
    ///     .header("My-Header", "Some special value")?;
    /// // ...
    /// # Ok(())
    /// # }
    /// ```
    pub fn header<'a>(
        mut self,
        name: impl Into<Cow<'a, str>>,
        val: impl Into<String>,
    ) -> crate::Result<Self> {
        self.headers
            .insert(headers::normalize_request_header(name)?, val.into());
        Ok(self)
    }

    /// Set this request's body.
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Set body to a json-serialized value and the the Content-Type header to "application/json".
    ///
    /// Returns an [crate::Error::SerializationError] if there was there was an error serializing the data.
    pub fn json(mut self, val: &(impl serde::Serialize + ?Sized)) -> Result<Self> {
        self.body = Some(serde_json::to_vec(val)?);
        self.headers.insert(
            headers::CONTENT_TYPE.to_owned(),
            "application/json".to_owned(),
        );
        Ok(self)
    }
}
