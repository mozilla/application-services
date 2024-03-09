/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A SettingsClient that uses the file-system. Used for developer ergonomics
//! (eg, for testing against experiments which are not deployed anywhere) and
//! for tests.

use crate::error::{info, warn, Result};
use crate::stateful::client::SettingsClient;
use crate::Experiment;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

pub struct FileSystemClient {
    path: PathBuf,
}

impl FileSystemClient {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().into(),
        })
    }
}

impl SettingsClient for FileSystemClient {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        info!("reading experiments in {}", self.path.display());
        let mut res = Vec::new();
        // Skip directories and non .json files (eg, READMEs)
        let json_ext = Some(OsStr::new("json"));
        let filenames = self
            .path
            .read_dir()?
            .filter_map(Result::ok)
            .map(|c| c.path())
            .filter(|f| f.is_file() && f.extension() == json_ext);
        for child_path in filenames {
            let file = File::open(child_path.clone())?;
            let reader = BufReader::new(file);
            match serde_json::from_reader::<_, Experiment>(reader) {
                Ok(exp) => res.push(exp),
                Err(e) => {
                    warn!(
                        "Malformed experiment found! File {},  Error: {}",
                        child_path.display(),
                        e
                    );
                }
            }
        }
        Ok(res)
    }
}
