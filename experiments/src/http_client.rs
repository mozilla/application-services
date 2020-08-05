/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is a simple Http client that uses viaduct to retrieve experiment data from the server
//! Currently configured to use Kinto and the old schema, although that would change once we start
//! Working on the real Nimbus schema.

use super::Experiment;
use anyhow::Result;
use viaduct::{status_codes, Request, Response};

// Making this a trait so that we can mock those later.
pub(crate) trait SettingsClient {
    fn get_experiements_metadata(&self) -> Result<String>;
    fn get_experiments(&self) -> Result<Vec<Experiment>>;
}

pub struct Client {}

impl Client {
    #[allow(unused)]
    pub fn new() -> Self {
        unimplemented!();
    }

    #[allow(unused)]
    fn make_request(&self, request: Request) -> Result<Response> {
        let resp = request.send()?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            anyhow::bail!("Error in request: {}", resp.text())
        }
    }
}

impl SettingsClient for Client {
    fn get_experiements_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn get_experiments(&self) -> Result<Vec<Experiment>> {
        unimplemented!();
    }
}

// TODO: Add unit tests
