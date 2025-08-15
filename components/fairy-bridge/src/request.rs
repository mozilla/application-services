/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{headers, FairyBridgeError, Response, Result};
use pollster::FutureExt;
use std::borrow::Cow;
use std::collections::HashMap;
use url::Url;

// repr(C) so that it can be easily used with the C backend.
#[derive(uniffi::Enum, Clone, Copy)]
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

#[derive(uniffi::Record, Clone)]
pub struct Request {
    pub settings: RequestSettings,
    pub method: Method,
    pub url: Url,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, uniffi::Record, Clone)]
#[repr(C)]
pub struct RequestSettings {
    // Timeout for the entire request in ms (0 indicates no timeout).
    #[uniffi(default = 0)]
    pub timeout: u32,
    // Maximum amount of redirects to follow (0 means redirects are not allowed)
    #[uniffi(default = 10)]
    pub redirect_limit: u32,
}

impl Default for RequestSettings {
    fn default() -> Self {
        Self {
            timeout: 0,
            redirect_limit: 10,
        }
    }
}

uniffi::custom_type!(Url, String, {
    remote,
    try_lift: |val| Ok(Url::parse(&val)?),
    lower: |obj| obj.into(),
});

/// Http request
///
/// These are created using the builder pattern, then sent over the network using the `send()`
/// method.
impl Request {
    pub fn new(settings: RequestSettings, method: Method, url: Url) -> Self {
        Self {
            settings,
            method,
            url,
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
    /// # use fairy_bridge::{Method, Request, RequestSettings, headers};
    /// # use url::Url;
    /// # fn main() -> fairy_bridge::Result<()> {
    /// # let some_url = url::Url::parse("https://www.example.com").unwrap();
    /// Request::new(RequestSettings::default(), Method::Post, some_url)
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
