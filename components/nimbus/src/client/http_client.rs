/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is a simple HTTP client that uses viaduct to retrieve experiment data from the server.
//! Currently configured to use Kinto and the old schema, although that would change once we start
//! working on the real Nimbus schema.
//!
//! In the future we might replace this with a more fully-feature Remote Settings client, such as:
//!
//!   https://github.com/mozilla-services/remote-settings-client
//!   Issue: https://github.com/mozilla/application-services/issues/3475
//!
//! But the simple subset implemented here meets our needs for now.

use std::time::{Duration, Instant};

use crate::config::RemoteSettingsConfig;
use crate::error::{NimbusError, Result};
use crate::{Experiment, SettingsClient, SCHEMA_VERSION};
use std::cell::Cell;
use url::Url;
use viaduct::{status_codes, Request, Response};

const HEADER_BACKOFF: &str = "Backoff";
const HEADER_RETRY_AFTER: &str = "Retry-After";

pub struct Client {
    pub(crate) base_url: Url,
    pub(crate) collection_name: String,
    pub(crate) remote_state: Cell<RemoteState>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RemoteState {
    Ok,
    Backoff {
        observed_at: Instant,
        duration: Duration,
    },
}

impl Client {
    #[allow(unused)]
    pub fn new(config: RemoteSettingsConfig) -> Result<Self> {
        let base_url = Url::parse(&config.server_url)?;
        Ok(Self {
            base_url,
            collection_name: config.collection_name,
            remote_state: Cell::new(RemoteState::Ok),
        })
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        self.ensure_no_backoff()?;
        let resp = request.send()?;
        self.handle_backoff_hint(&resp)?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            Err(NimbusError::ResponseError(resp.text().to_string()))
        }
    }

    fn ensure_no_backoff(&self) -> Result<()> {
        if let RemoteState::Backoff {
            observed_at,
            duration,
        } = self.remote_state.get()
        {
            let elapsed_time = observed_at.elapsed();
            if elapsed_time >= duration {
                self.remote_state.replace(RemoteState::Ok);
            } else {
                let remaining = duration - elapsed_time;
                return Err(NimbusError::BackoffError(remaining.as_secs()));
            }
        }
        Ok(())
    }

    fn handle_backoff_hint(&self, response: &Response) -> Result<()> {
        let extract_backoff_header = |header| -> Result<u64> {
            Ok(response
                .headers
                .get_as::<u64, _>(header)
                .transpose()
                .unwrap_or_default() // Ignore number parsing errors.
                .unwrap_or(0))
        };
        // In practice these two headers are mutually exclusive.
        let backoff = extract_backoff_header(HEADER_BACKOFF)?;
        let retry_after = extract_backoff_header(HEADER_RETRY_AFTER)?;
        let max_backoff = backoff.max(retry_after);

        if max_backoff > 0 {
            self.remote_state.replace(RemoteState::Backoff {
                observed_at: Instant::now(),
                duration: Duration::from_secs(max_backoff),
            });
        }
        Ok(())
    }
}

impl SettingsClient for Client {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        let path = format!(
            "v1/buckets/main/collections/{}/records",
            &self.collection_name
        );
        let url = self.base_url.join(&path)?;
        let req = Request::get(url);
        let resp = self.make_request(req)?;
        parse_experiments(&resp.text())
    }
}

pub fn parse_experiments(payload: &str) -> Result<Vec<Experiment>> {
    // We first encode the response into a `serde_json::Value`
    // to allow us to deserialize each experiment individually,
    // omitting any malformed experiments
    let value: serde_json::Value = serde_json::from_str(payload)?;
    let data = value
        .get("data")
        .ok_or(NimbusError::InvalidExperimentFormat)?;
    let mut res = Vec::new();
    for exp in data
        .as_array()
        .ok_or(NimbusError::InvalidExperimentFormat)?
    {
        // Validate the schema major version matches the supported version
        let exp_schema_version = match exp.get("schemaVersion") {
            Some(ver) => {
                serde_json::from_value::<String>(ver.to_owned()).unwrap_or_else(|_| "".to_string())
            }
            None => {
                log::trace!("Missing schemaVersion: {:#?}", exp);
                continue;
            }
        };
        let schema_maj_version = exp_schema_version.split('.').next().unwrap_or("");
        // While "0" is a valid schema version, we have already passed that so reserving zero as
        // a special value here in order to avoid a panic, and just ignore the experiment.
        let schema_version: u32 = schema_maj_version.parse().unwrap_or(0);
        if schema_version != SCHEMA_VERSION {
            log::info!(
                    "Schema version mismatch: Expected version {}, discarding experiment with version {}",
                    SCHEMA_VERSION, schema_version
                );
            // Schema version mismatch
            continue;
        }

        match serde_json::from_value::<Experiment>(exp.clone()) {
            Ok(exp) => res.push(exp),
            Err(e) => {
                log::trace!("Malformed experiment data: {:#?}", exp);
                log::warn!(
                    "Malformed experiment found! Experiment {},  Error: {}",
                    exp.get("id").unwrap_or(&serde_json::json!("ID_NOT_FOUND")),
                    e
                );
            }
        }
    }
    Ok(res)
}
