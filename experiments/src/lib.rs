// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod error;
mod evaluator;
pub use error::*;
mod http_client;
mod matcher;
mod persistence;

use anyhow::{anyhow, Result};
use serde_derive::*;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Experiment {
    slug: String,
}
/// Experiments is the main struct representing the experiements state
/// It should hold all the information needed to communcate a specific user's
/// Experiementation status (note: This should have some type of uuid)
#[derive(Debug, Clone)]
pub struct Experiments {
    experiments: Vec<Experiment>,
}

impl Default for Experiments {
    fn default() -> Self {
        Experiments::new()
    }
}

impl Experiments {
    pub fn new() -> Self {
        let resp = vec![];
        Self { experiments: resp }
    }

    pub fn get_experiment_branch(&self) -> Result<String> {
        Err(anyhow!("Not implemented"))
    }

    pub fn get_experiments(&self) -> &Vec<Experiment> {
        &self.experiments
    }
}
