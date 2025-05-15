/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Forked from https://github.com/thomcc/find-places-dbs, which was itself
// originally in app-services, then split out for general-purpose use, but
// then became unmaintained and published to crates.io without a Mozilla owner.
// If we ever need to split it out again we should contact Thom, who I'm sure
// would be happy to give us ownership of the crate.

use anyhow::{bail, format_err, Result};
use error_support::{debug, info, trace, warn};
use std::{fs, path::PathBuf, process};

#[derive(Clone, Debug, PartialEq)]
pub struct PlacesLocation {
    pub profile_name: String,
    pub path: PathBuf,
    pub db_size: u64,
}

impl PlacesLocation {
    pub fn friendly_db_size(&self) -> String {
        let sizes = [
            (1024 * 1024 * 1024, "Gb"),
            (1024 * 1024, "Mb"),
            (1024, "Kb"),
        ];
        for (lim, suffix) in &sizes {
            if self.db_size >= *lim {
                return format!(
                    "~{} {}",
                    ((self.db_size as f64 / *lim as f64) * 10.0).round() / 10.0,
                    suffix
                );
            }
        }
        format!("{} bytes", self.db_size)
    }
}

pub fn get_all_places_dbs() -> Result<Vec<PlacesLocation>> {
    let mut path = match dirs::home_dir() {
        Some(dir) => dir,
        None => bail!("No home directory found!"),
    };
    if cfg!(windows) {
        path.extend(&["AppData", "Roaming", "Mozilla", "Firefox", "Profiles"]);
    } else {
        let out = String::from_utf8(process::Command::new("uname").args(["-s"]).output()?.stdout)?;
        info!("Uname says: {:?}", out);
        if out.trim() == "Darwin" {
            // ~/Library/Application Support/Firefox/Profiles
            path.extend(&["Library", "Application Support", "Firefox", "Profiles"]);
        } else {
            // I'm not actually sure if this is true for all non-macos unix likes.
            path.extend(&[".mozilla", "firefox"]);
        }
    }
    debug!("Using profile path: {:?}", path);
    let mut res = fs::read_dir(path)?
        .map(|entry_result| {
            let entry = entry_result?;
            trace!("Considering path {:?}", entry.path());
            if !entry.path().is_dir() {
                trace!("  Not dir: {:?}", entry.path());
                return Ok(None);
            }
            let mut path = entry.path().to_owned();
            let profile_name = path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .ok_or_else(|| {
                    warn!("  Path has invalid UTF8: {:?}", path);
                    format_err!("Path has invalid UTF8: {:?}", path)
                })?
                .into();
            path.push("places.sqlite");
            if !path.exists() {
                return Ok(None);
            }
            let metadata = fs::metadata(&path)?;
            let db_size = metadata.len();
            Ok(Some(PlacesLocation {
                profile_name,
                path,
                db_size,
            }))
        })
        .filter_map(|result: Result<Option<PlacesLocation>>| match result {
            Ok(val) => val,
            Err(e) => {
                debug!("Got error finding profile directory, skipping: {}", e);
                None
            }
        })
        .collect::<Vec<_>>();
    res.sort_by(|a, b| b.db_size.cmp(&a.db_size));
    Ok(res)
}

pub fn get_largest_places_db() -> Result<Option<PlacesLocation>> {
    let all = get_all_places_dbs()?;
    if all.is_empty() {
        warn!("No places dbs!");
        return Ok(None);
    }
    // Already sorted by size (descending)
    Ok(Some(all.into_iter().next().unwrap()))
}
