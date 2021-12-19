/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use crate::{backend::Backend, settings::GLOBAL_SETTINGS};
use crate::{Error, Fetcher, Method, Response};
use once_cell::sync::OnceCell;
use url::Url;

static FETCHER: OnceCell<Box<dyn Fetcher>> = OnceCell::new();

macro_rules! backend_error {
    ($($args:tt)*) => {{
        let msg = format!($($args)*);
        log::error!("{}", msg);
        Error::BackendError(msg)
    }};
}

pub struct FfiRequest {
    pub method: Method,
    pub url: Url,
    pub body: Option<Vec<i8>>,
    pub headers: HashMap<String, String>,
    pub follow_redirects: bool,
    pub use_caches: bool,
    pub connect_timeout_secs: i32,
    pub read_timeout_secs: i32,
}

pub enum FfiResponse {
    Err {
        message: String,
    },
    Ok {
        url: Url,
        status: i32,
        body: Option<Vec<i8>>,
        headers: HashMap<String, String>,
        req_method: Method,
    },
}

impl TryFrom<FfiResponse> for Response {
    type Error = Error;
    fn try_from(response: FfiResponse) -> std::result::Result<Self, Self::Error> {
        match response {
            FfiResponse::Err { message } => Err(Error::NetworkError(format!(
                "Error on network request: {}",
                message
            ))),
            FfiResponse::Ok {
                status,
                headers,
                url,
                body,
                req_method,
            } => {
                if status < 0 || status > i32::from(u16::max_value()) {
                    return Err(backend_error!("Illegal HTTP status: {}", status));
                }
                let mut ir_headers = crate::Headers::with_capacity(headers.len());
                for (name, val) in headers {
                    let hname = match crate::HeaderName::new(name) {
                        Ok(name) => name,
                        Err(e) => {
                            // Ignore headers with invalid names, since nobody can look for them anyway.
                            log::warn!("Server sent back invalid header name: '{}'", e);
                            continue;
                        }
                    };
                    // Not using Header::new since the error it returns is for request headers.
                    ir_headers.insert_header(crate::Header::new_unchecked(hname, val));
                }
                Ok(Self {
                    url,
                    request_method: req_method,
                    body: body
                        .unwrap_or_default()
                        .into_iter()
                        .map(|b| b as u8)
                        .collect::<Vec<_>>(),
                    status: status as u16,
                    headers: ir_headers,
                })
            }
        }
    }
}
impl From<crate::Request> for FfiRequest {
    fn from(request: crate::Request) -> Self {
        Self {
            method: request.method,
            url: request.url,
            body: request
                .body
                .map(|inner| inner.into_iter().map(|b| b as i8).collect::<Vec<_>>()),
            headers: request.headers.into(),
            follow_redirects: GLOBAL_SETTINGS.follow_redirects,
            use_caches: GLOBAL_SETTINGS.use_caches,
            connect_timeout_secs: GLOBAL_SETTINGS
                .connect_timeout
                .map_or(0, |d| d.as_secs() as i32),
            read_timeout_secs: GLOBAL_SETTINGS
                .read_timeout
                .map_or(0, |d| d.as_secs() as i32),
        }
    }
}

#[derive(Default)]
pub struct FfiBackend;

impl FfiBackend {
    pub fn new() -> Self {
        Self {}
    }
    pub fn set_backend(&self, fetch: Box<dyn Fetcher>) {
        if FETCHER.set(fetch).is_err() {
            log::warn!("viaduct http request backend already set");
        }
    }
}

impl Backend for FfiBackend {
    fn send(&self, request: crate::Request) -> Result<crate::Response, Error> {
        super::note_backend("FFI (trusted)");
        let fetcher = FETCHER.get().ok_or(Error::BackendNotInitialized)?;
        fetcher.fetch(request.into()).try_into()
    }
}
