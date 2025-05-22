/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::error::{trace, Error};
use super::models::{CuratedRecommendationsRequest, CuratedRecommendationsResponse};
use url::Url;
use viaduct::{header_names, Request, Response};

pub struct HttpClient;

impl HttpClient {
    pub fn make_curated_recommendation_request(
        &self,
        request: &CuratedRecommendationsRequest,
        user_agent_header: &str,
        url: Url,
    ) -> Result<CuratedRecommendationsResponse, Error> {
        trace!("making request: {url}");
        let response: Response = Request::post(url)
            .header(header_names::ACCEPT, "application/json")?
            .header(header_names::USER_AGENT, user_agent_header)?
            .json(request)
            .send()?;

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

        let response_json: CuratedRecommendationsResponse = response.json()?;
        trace!("response: {}", response.text());
        Ok(response_json)
    }
}

pub trait HttpClientTrait {
    fn make_curated_recommendation_request(
        &self,
        request: &CuratedRecommendationsRequest,
        user_agent_header: &str,
        url: Url,
    ) -> super::error::Result<CuratedRecommendationsResponse>;
}

impl HttpClientTrait for HttpClient {
    fn make_curated_recommendation_request(
        &self,
        request: &CuratedRecommendationsRequest,
        user_agent_header: &str,
        url: Url,
    ) -> super::error::Result<CuratedRecommendationsResponse> {
        self.make_curated_recommendation_request(request, user_agent_header, url)
    }
}
