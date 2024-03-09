/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{error, warn};
use once_cell::sync::Lazy;
use std::{io::Read, sync::Once};
use viaduct::{settings::GLOBAL_SETTINGS, Backend};

// Note: we don't `use` things from reqwest or the viaduct crate because
// it would be rather confusing given that we have the same name for
// most things as them.

static CLIENT: Lazy<reqwest::blocking::Client> = Lazy::new(|| {
    let settings = GLOBAL_SETTINGS.read();
    let mut builder = reqwest::blocking::ClientBuilder::new()
        .timeout(settings.read_timeout)
        .connect_timeout(settings.connect_timeout)
        .redirect(if settings.follow_redirects {
            reqwest::redirect::Policy::default()
        } else {
            reqwest::redirect::Policy::none()
        });
    if cfg!(target_os = "ios") {
        // The FxA servers rely on the UA agent to filter
        // some push messages directed to iOS devices.
        // This is obviously a terrible hack and we should
        // probably do https://github.com/mozilla/application-services/issues/1326
        // instead, but this will unblock us for now.
        builder = builder.user_agent("Firefox-iOS-FxA/24");
    }
    // Note: no cookie or cache support.
    builder
        .build()
        .expect("Failed to initialize global reqwest::Client")
});

#[allow(clippy::unnecessary_wraps)] // not worth the time to untangle
fn into_reqwest(request: viaduct::Request) -> Result<reqwest::blocking::Request, viaduct::Error> {
    let method = match request.method {
        viaduct::Method::Get => reqwest::Method::GET,
        viaduct::Method::Head => reqwest::Method::HEAD,
        viaduct::Method::Post => reqwest::Method::POST,
        viaduct::Method::Put => reqwest::Method::PUT,
        viaduct::Method::Delete => reqwest::Method::DELETE,
        viaduct::Method::Connect => reqwest::Method::CONNECT,
        viaduct::Method::Options => reqwest::Method::OPTIONS,
        viaduct::Method::Trace => reqwest::Method::TRACE,
        viaduct::Method::Patch => reqwest::Method::PATCH,
    };
    let mut result = reqwest::blocking::Request::new(method, request.url);
    for h in request.headers {
        use reqwest::header::{HeaderName, HeaderValue};
        // Unwraps should be fine, we verify these in `Header`
        let value = HeaderValue::from_str(h.value()).unwrap();
        result
            .headers_mut()
            .insert(HeaderName::from_bytes(h.name().as_bytes()).unwrap(), value);
    }
    *result.body_mut() = request.body.map(reqwest::blocking::Body::from);
    Ok(result)
}

pub struct ReqwestBackend;
impl Backend for ReqwestBackend {
    fn send(&self, request: viaduct::Request) -> Result<viaduct::Response, viaduct::Error> {
        viaduct::note_backend("reqwest (untrusted)");
        let request_method = request.method;
        let req = into_reqwest(request)?;
        let mut resp = CLIENT
            .execute(req)
            .map_err(|e| viaduct::Error::NetworkError(e.to_string()))?;
        let status = resp.status().as_u16();
        let url = resp.url().clone();
        let mut body = Vec::with_capacity(resp.content_length().unwrap_or_default() as usize);
        resp.read_to_end(&mut body).map_err(|e| {
            error!("Failed to get body from response: {:?}", e);
            viaduct::Error::NetworkError(e.to_string())
        })?;
        let mut headers = viaduct::Headers::with_capacity(resp.headers().len());
        for (k, v) in resp.headers() {
            let val = String::from_utf8_lossy(v.as_bytes()).to_string();
            let hname = match viaduct::HeaderName::new(k.as_str().to_owned()) {
                Ok(name) => name,
                Err(e) => {
                    // Ignore headers with invalid names, since nobody can look for them anyway.
                    warn!("Server sent back invalid header name: '{}'", e);
                    continue;
                }
            };
            // Not using Header::new since the error it returns is for request headers.
            headers.insert_header(viaduct::Header::new_unchecked(hname, val));
        }
        Ok(viaduct::Response {
            request_method,
            url,
            status,
            headers,
            body,
        })
    }
}

static INIT_REQWEST_BACKEND: Once = Once::new();

pub fn use_reqwest_backend() {
    INIT_REQWEST_BACKEND.call_once(|| {
        viaduct::set_backend(Box::leak(Box::new(ReqwestBackend)))
            .expect("Backend already set (FFI)");
    })
}

#[no_mangle]
#[cfg(target_os = "ios")]
pub extern "C" fn viaduct_use_reqwest_backend() {
    use_reqwest_backend();
}
