/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "places")]
use places::PlacesApi;
// use sql_support::SqlInterruptHandle;
use crate::error::*;
use crate::msg_types::{SyncParams, SyncResult};
#[cfg(feature = "logins")]
use logins::PasswordEngine;
#[cfg(feature = "logins")]
use std::sync::Mutex;
#[cfg(any(feature = "places", feature = "logins"))]
use std::sync::{Arc, Weak};

pub struct SyncManager {
    #[cfg(feature = "places")]
    places: Weak<PlacesApi>,
    #[cfg(feature = "logins")]
    logins: Weak<Mutex<PasswordEngine>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "places")]
            places: Weak::new(),
            #[cfg(feature = "logins")]
            logins: Weak::new(),
        }
    }

    #[cfg(feature = "places")]
    pub fn set_places(&mut self, places: Arc<PlacesApi>) {
        self.places = Arc::downgrade(&places);
    }

    #[cfg(feature = "logins")]
    pub fn set_logins(&mut self, logins: Arc<Mutex<PasswordEngine>>) {
        self.logins = Arc::downgrade(&logins);
    }

    pub fn disconnect(&mut self) {
        unimplemented!();
    }

    pub fn sync(&mut self, params: SyncParams) -> Result<SyncResult> {
        check_engine_list(&params.engines_to_sync)?;
        unimplemented!();
    }
}

fn check_engine_list(list: &[String]) -> Result<()> {
    for e in list {
        if e == "bookmarks" || e == "history" {
            if cfg!(not(feature = "places")) {
                return Err(ErrorKind::UnsupportedFeature(e.to_string()).into());
            }
            continue;
        }
        if e == "passwords" {
            if cfg!(not(feature = "logins")) {
                return Err(ErrorKind::UnsupportedFeature(e.to_string()).into());
            }
            continue;
        }
        return Err(ErrorKind::UnknownEngine(e.to_string()).into());
    }
    Ok(())
}
