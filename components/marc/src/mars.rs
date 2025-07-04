/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::{
    error::{check_http_status_for_error, Error, Result},
    models::{AdRequest, AdResponse},
};
use url::Url;
use uuid::Uuid;
use viaduct::Request;

const DEFAULT_MARS_API_ENDPOINT: &str = "https://ads.allizom.org/v1";

pub trait MARSClient: Sync + Send {
    fn fetch_ads(&self, request: &AdRequest) -> Result<AdResponse>;
    fn record_impression(&self, url_callback_string: Option<&String>) -> Result<()>;
    fn record_click(&self, url_callback_string: Option<&String>) -> Result<()>;
    fn record_report_ad(&self, url_callback_string: Option<&String>) -> Result<()>;
    fn get_context_id(&self) -> &str;
    fn cycle_context_id(&mut self) -> String;
    fn get_mars_endpoint(&self) -> String {
        DEFAULT_MARS_API_ENDPOINT.to_string()
    }
}

pub struct DefaultMARSClient {
    context_id: String,
}

impl DefaultMARSClient {
    pub fn new(context_id: String) -> Self {
        Self { context_id }
    }
    fn make_callback_request(&self, url_callback_string: &str) -> Result<()> {
        let request = Request::get(Url::parse(url_callback_string)?);
        let response = request.send()?;
        check_http_status_for_error(&response)
    }
}

impl MARSClient for DefaultMARSClient {
    fn get_context_id(&self) -> &str {
        &self.context_id
    }
    /// Updates the client's context_id to the passed value and returns the previous context_id
    fn cycle_context_id(&mut self) -> String {
        let old_context_id = self.context_id.clone();
        self.context_id = Uuid::new_v4().to_string();
        old_context_id
    }
    fn fetch_ads(&self, ad_request: &AdRequest) -> Result<AdResponse> {
        let endpoint = self.get_mars_endpoint();
        let url = Url::parse(&format!("{endpoint}/ads"))?;

        let request = Request::post(url).json(ad_request);
        let response = request.send()?;

        check_http_status_for_error(&response)?;

        let response_json: AdResponse = response.json()?;
        Ok(response_json)
    }
    fn record_impression(&self, url_callback_string: Option<&String>) -> Result<()> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(callback),
            None => Err(Error::MissingCallback {
                message: "Impression callback url empty.".to_string(),
            }),
        }
    }
    fn record_click(&self, url_callback_string: Option<&String>) -> Result<()> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(callback),
            None => Err(Error::MissingCallback {
                message: "Click callback url empty.".to_string(),
            }),
        }
    }
    fn record_report_ad(&self, url_callback_string: Option<&String>) -> Result<()> {
        match url_callback_string {
            Some(callback) => self.make_callback_request(callback),
            None => Err(Error::MissingCallback {
                message: "Report callback url empty.".to_string(),
            }),
        }
    }
}
