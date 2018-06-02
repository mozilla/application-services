/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use hawk::{Credentials, Digest, Key, PayloadHasher, RequestBuilder};
use hex;
use reqwest::{header, Client, Method, Request};
use serde_json;
use url::Url;

use errors::*;

const KEY_LENGTH: usize = 32;

pub struct FxAHAWKRequestBuilder<'a> {
    url: Url,
    method: Method,
    body: Option<String>,
    hkdf_sha256_key: &'a Vec<u8>,
}

impl<'a> FxAHAWKRequestBuilder<'a> {
    pub fn new(method: Method, url: Url, hkdf_sha256_key: &'a Vec<u8>) -> Self {
        FxAHAWKRequestBuilder {
            url,
            method,
            body: None,
            hkdf_sha256_key,
        }
    }

    // This class assumes that the content being sent it always of the type
    // application/json.
    pub fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body.to_string());
        self
    }

    pub fn build(self) -> Result<Request> {
        let hawk_header;
        {
            // Make sure we de-allocate the hash after hawk_request_builder.
            let hash;
            let method = format!("{}", self.method);
            let mut hawk_request_builder = RequestBuilder::from_url(method.as_str(), &self.url)?;
            if let Some(ref body) = self.body {
                hash = PayloadHasher::hash("application/json", Digest::sha256(), &body)?;
                hawk_request_builder = hawk_request_builder.hash(&hash[..]);
            }
            let hawk_request = hawk_request_builder.request();
            let token_id = hex::encode(&self.hkdf_sha256_key[0..KEY_LENGTH]);
            let hmac_key = &self.hkdf_sha256_key[KEY_LENGTH..(2 * KEY_LENGTH)];
            let hawk_credentials = Credentials {
                id: token_id,
                key: Key::new(hmac_key, Digest::sha256())?,
            };
            let header = hawk_request.make_header(&hawk_credentials)?;
            hawk_header = format!("Hawk {}", header);
        }

        let mut request_builder = Client::new().request(self.method, self.url);
        request_builder.header(header::Authorization(hawk_header));

        if let Some(body) = self.body {
            request_builder.header(header::ContentType::json());
            request_builder.body(body);
        }

        Ok(request_builder.build()?)
    }
}
