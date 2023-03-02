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

use crate::error::{NimbusError, Result};
use crate::{Experiment, SettingsClient, SCHEMA_VERSION};
use rs_client::Client;

impl SettingsClient for Client {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        let resp = self.get_records()?;
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
