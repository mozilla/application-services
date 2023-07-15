// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::{
    sources::{ExperimentListSource, ExperimentSource},
    value_utils::{self, CliUtils},
    NimbusApp,
};

impl NimbusApp {
    pub(crate) fn fetch_list<P>(&self, list: &ExperimentListSource, file: Option<P>) -> Result<bool>
    where
        P: AsRef<Path>,
    {
        let value: Value = list.try_into()?;
        let array = value_utils::try_extract_data_list(&value)?;
        let mut data = Vec::new();
        let filter = self.app_name.as_deref();
        for exp in array {
            let app_name = exp.get_str("appName").unwrap_or_default();
            if filter.is_some() && Some(app_name) != filter {
                continue;
            }

            data.push(exp);
        }
        write_experiments_to_file(&data, file)?;
        Ok(true)
    }
}

impl NimbusApp {
    pub(crate) fn fetch_recipes<P>(
        &self,
        recipes: &Vec<ExperimentSource>,
        file: Option<P>,
    ) -> Result<bool>
    where
        P: AsRef<Path>,
    {
        let mut data = Vec::new();

        let filter = self.app_name.as_deref();
        for exp in recipes {
            let exp: Value = exp.try_into()?;
            let app_name = exp.get_str("appName").unwrap_or_default();
            if filter.is_some() && Some(app_name) != filter {
                continue;
            }

            data.push(exp);
        }

        write_experiments_to_file(&data, file)?;
        Ok(true)
    }
}

fn write_experiments_to_file<P>(data: &Vec<Value>, file: Option<P>) -> Result<()>
where
    P: AsRef<Path>,
{
    let contents = serde_json::json!({
        "data": data,
    });
    value_utils::write_to_file_or_print(file, &contents)?;
    Ok(())
}
