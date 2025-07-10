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
    use nimbus::{NimbusClient, RemoteSettingsConfig, RemoteSettingsServer};
    use std::path::PathBuf;
    use url::Url;

    error_support::init_for_tests();

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("tests/experiments");

    let url = Url::from_file_path(dir).expect("experiments dir should exist");

    let config = RemoteSettingsConfig {
        server: Some(RemoteSettingsServer::Custom {
            url: url.as_str().to_string(),
        }),
        server_url: None,
        bucket_name: None,
        collection_name: "doesn't matter".to_string(),
    };

    let tmp_dir = tempfile::tempdir()?;
    let client = NimbusClient::new(
        Default::default(),
        Default::default(),
        Default::default(),
        tmp_dir.path(),
        Some(config),
        Box::new(NoopMetricsHandler),
        None,
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
