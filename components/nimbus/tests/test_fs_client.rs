/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
#![cfg(feature = "rkv-safe-mode")]

// Simple tests for our file-system client

use nimbus::error::Result;

mod common;

// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.
#[test]
fn test_simple() -> Result<()> {
    use common::NoopMetricsHandler;
    use nimbus::{NimbusClient, RemoteSettingsConfig};
    use std::path::PathBuf;
    use url::Url;

    let _ = env_logger::try_init();

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("tests/experiments");

    let url = Url::from_file_path(dir).expect("experiments dir should exist");

    let config = RemoteSettingsConfig {
        server_url: Some(url.as_str().to_string()),
        bucket_name: None,
        collection_name: "doesn't matter".to_string(),
    };

    let tmp_dir = tempfile::tempdir()?;
    let aru = Default::default();
    let client = NimbusClient::new(
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        Some(config),
        aru,
        Box::new(NoopMetricsHandler),
    )?;
    client.fetch_experiments()?;
    client.apply_pending_experiments()?;

    let experiments = client.get_all_experiments()?;
    assert_eq!(experiments.len(), 1);
    assert_eq!(experiments[0].slug, "secure-gold");
    // Once we can set the nimbus ID, we should set it to a uuid we know
    // gets enrolled.
    Ok(())
}
