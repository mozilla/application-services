/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::error::Error;
use crate::{
    error::ApiResult,
    models::{AdRequest, AdResponse},
};
use error_support::handle_error;
use url::Url;
use viaduct::Request;

const DEFAULT_MARS_API_ENDPOINT: &str = "https://ads.allizom.org/v1/";

#[derive(uniffi::Object)]
pub struct MARSClient;

#[uniffi::export]
impl MARSClient {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    #[handle_error(Error)]
    pub fn request_ad(&self, ad_request: &AdRequest) -> ApiResult<AdResponse> {
        let url = Url::parse(&format!("{DEFAULT_MARS_API_ENDPOINT}ads"))?;

        let request: Request = Request::post(url).json(ad_request);

        let response = request.send()?;

        let status = response.status;
        if status >= 400 {
            let error_message = response.text();
            let error = match status {
                400 => Error::BadRequest {
                    code: status,
                    message: error_message.to_string(),
                },
                422 => Error::Validation {
                    code: status,
                    message: error_message.to_string(),
                },
                500..=599 => Error::Server {
                    code: status,
                    message: error_message.to_string(),
                },
                _ => Error::Unexpected {
                    code: status,
                    message: error_message.to_string(),
                },
            };
            return Err(error);
        }

        let response_json: AdResponse = response.json()?;
        Ok(response_json)
    }
}

#[cfg(test)]
mod tests {
    use crate::mars::MARSClient;
    use crate::models::{AdPlacement, AdRequest};

    #[test]
    fn mars_client_can_call() {
        viaduct_reqwest::use_reqwest_backend();
        let ad_request = AdRequest {
            context_id: "03267ad1-0074-4aa6-8e0c-ec18e0906bfe".to_string(),
            placements: vec![AdPlacement {
                placement: "pocket_billboard_1".to_string(),
                count: 1,
                content: None,
            }],
        };

        let client = MARSClient::new();
        let resp = client.request_ad(&ad_request);

        match resp {
            Ok(v) => println!("{:?}", v),
            Err(v) => println!("Error {:?}", v),
        }
    }
}
