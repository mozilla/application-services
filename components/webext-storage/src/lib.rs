/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

mod api;
pub mod db;
pub mod error;
pub mod repo;
mod schema;

// This is what we roughly expect the "bridge" used by desktop to do.
// It's primarily here to avoid dead-code warnings (but I don't want to disable
// those warning, as stuff that remains after this is suspect!)
pub fn delme_demo_usage() -> error::Result<()> {
    use serde_json::json;

    let repo = repo::Repo::new("webext-storage.db")?;
    repo.set("ext-id", json!({}))?;
    repo.get("ext-id", json!({}))?;
    repo.remove("ext-id", json!({}))?;
    repo.clear("ext-id")?;
    Ok(())
}
