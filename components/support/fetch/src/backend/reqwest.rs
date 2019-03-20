/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::settings::GLOBAL_SETTINGS;

// Note: we don't `use` things from reqwest or this crate because it
// would be rather confusing given that we have the same name for
// most things as them.

lazy_static::lazy_static! {
    static ref CLIENT: reqwest::Client = {
        reqwest::ClientBuilder::new()
            .timeout(GLOBAL_SETTINGS.read_timeout)
            .connect_timeout(GLOBAL_SETTINGS.connect_timeout)
            .redirect(
                if GLOBAL_SETTINGS.follow_redirects {
                    reqwest::RedirectPolicy::default()
                } else {
                    reqwest::RedirectPolicy::none()
                }
            )
            // Note: no cookie or cache support.
            .build()
            .expect("Failed to initialize global reqwest::Client")
    };
}

// Implementing From to do this would end up being public
impl crate::Request {
    fn into_reqwest(self) -> Result<reqwest::Request, crate::Error> {
        let method = match self.method {
            crate::Method::Get => reqwest::Method::GET,
            crate::Method::Head => reqwest::Method::HEAD,
            crate::Method::Post => reqwest::Method::POST,
            crate::Method::Put => reqwest::Method::PUT,
            crate::Method::Delete => reqwest::Method::DELETE,
            crate::Method::Connect => reqwest::Method::CONNECT,
            crate::Method::Options => reqwest::Method::OPTIONS,
            crate::Method::Trace => reqwest::Method::TRACE,
        };
        let mut result = reqwest::Request::new(method, self.url);
        for h in self.headers {
            use reqwest::header::{HeaderName, HeaderValue};
            let value = HeaderValue::from_str(&h.value).map_err(|_| {
                crate::Error::BackendError(format!("Invalid header value for header '{}'", h.name))
            })?;
            // Unwrap should be fine, we check this in our Headers type.
            result
                .headers_mut()
                .insert(HeaderName::from_bytes(h.name.as_bytes()).unwrap(), value);
        }
        *result.body_mut() = self.body.map(reqwest::Body::from);
        Ok(result)
    }
}

pub fn send(request: crate::Request) -> Result<crate::Response, crate::Error> {
    let request_method = request.method;
    let req = request.into_reqwest()?;
    let mut resp = CLIENT.execute(req).map_err(|e| {
        log::error!("Reqwest error: {:?}", e);
        crate::Error::NetworkError(e.to_string())
    })?;
    let status = resp.status().as_u16();
    let url = resp.url().clone();
    let body_text = resp.text().map_err(|e| {
        log::error!("Failed to get body from response: {:?}", e);
        crate::Error::NetworkError(e.to_string())
    })?;
    let mut headers = crate::Headers::with_capacity(resp.headers().len());
    for (k, v) in resp.headers() {
        let val = std::str::from_utf8(v.as_bytes()).map_err(|_| {
            log::error!("Server sent back non-utf8 value for header '{}'", k);
            crate::Error::NetworkError(format!("Non UTF-8 data in header '{}'", k))
        })?;
        let hname = crate::HeaderName::new(k.as_str().to_owned()).map_err(|_| {
            log::error!("Server sent back invalid header name: '{}'", k);
            crate::Error::NetworkError(format!("Illegal header name '{}'", k))
        })?;
        headers.insert(hname, val);
    }
    Ok(crate::Response {
        request_method,
        body_text,
        url,
        status,
        headers,
    })
}
