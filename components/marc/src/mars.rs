/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::collections::HashMap;

use super::error::Error;
use crate::{
    error::ApiResult,
    models::{self, AdRequest, AdResponse},
    MozAdsPlacement, MozAdsPlacementConfig,
};
use error_support::handle_error;
use url::Url;
use viaduct::Request;

const DEFAULT_MARS_API_ENDPOINT: &str = "https://ads.allizom.org/v1";

#[derive(uniffi::Object)]
pub struct MARSClient;

#[uniffi::export]
impl MARSClient {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    #[handle_error(Error)]
    pub fn request_ads(
        &self,
        ad_configs: &Vec<MozAdsPlacementConfig>,
    ) -> ApiResult<HashMap<String, MozAdsPlacement>> {
        let request = build_request_from_placement_configs(ad_configs);

        let mars_response = self.request_ad_from_mars(&request);

        match mars_response {
            Ok(v) => Ok(build_placements(ad_configs, v)),
            Err(v) => {
                return Err(Error::Unexpected {
                    code: 500, // TODO: better error handling, these should not just be 500s
                    message: v.to_string(),
                });
            }
        }
    }

    #[handle_error(Error)]
    fn request_ad_from_mars(&self, ad_request: &AdRequest) -> ApiResult<AdResponse> {
        let url = Url::parse(&format!("{DEFAULT_MARS_API_ENDPOINT}/ads"))?;

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

fn build_request_from_placement_configs(
    placement_configs: &Vec<MozAdsPlacementConfig>,
) -> AdRequest {
    let mut request = AdRequest {
        placements: vec![],
        context_id: "03267ad1-0074-4aa6-8e0c-ec18e0906bfe".to_string(),
    };

    for placement_config in placement_configs {
        request.placements.push(models::AdPlacementRequest {
            placement: placement_config.placement_id.clone(),
            count: 1,
            content: None,
        });
    }

    request
}

fn build_placements(
    placement_configs: &Vec<MozAdsPlacementConfig>,
    mut mars_response: AdResponse,
) -> HashMap<String, MozAdsPlacement> {
    let mut moz_ad_placements: HashMap<String, MozAdsPlacement> = HashMap::new();

    for config in placement_configs {
        let placement_content = mars_response.data.get_mut(&config.placement_id);

        match placement_content {
            Some(v) => {
                let ad_content = v.pop();
                match ad_content {
                    Some(c) => {
                        let is_updated = moz_ad_placements.insert(
                            config.placement_id.clone(),
                            MozAdsPlacement {
                                content: c,
                                placement_config: config.clone(),
                            },
                        );
                        if let Some(v) = is_updated {
                            //TODO: Some error needs to occur if we have more than one instance of
                            //a placement_id
                            println!(
                                "Duplicate placement_id found: {:?}",
                                v.placement_config.placement_id
                            )
                        }
                    }
                    None => continue,
                }
            }
            None => continue,
        }
    }

    moz_ad_placements
}

#[cfg(test)]
mod tests {
    use crate::mars::MARSClient;
    use crate::MozAdsPlacementConfig;

    #[test]
    fn mars_client_call_with_formatting() {
        viaduct_reqwest::use_reqwest_backend();

        let ad_configs = vec![
            MozAdsPlacementConfig {
                placement_id: "pocket_billboard_1".to_string(),
                iab_content: None,
                fixed_size: None,
            },
            MozAdsPlacementConfig {
                placement_id: "pocket_billboard_2".to_string(),
                iab_content: None,
                fixed_size: None,
            },
        ];

        let client = MARSClient::new();
        let resp = client.request_ads(&ad_configs);

        match resp {
            Ok(v) => {
                println!("{:?}", v);
            }
            Err(v) => println!("Error {:?}", v),
        }
    }
}
