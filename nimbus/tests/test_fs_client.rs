/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Simple tests for our file-system client

use nimbus::{error::Result, AvailableRandomizationUnits, NimbusClient, RemoteSettingsConfig};
use std::path::PathBuf;
use tempdir::TempDir;
use url::Url;

#[test]
fn test_simple() -> Result<()> {
    let _ = env_logger::try_init();

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("tests/experiments");

    let url = Url::from_file_path(dir).expect("experiments dir should exist");

    let config = RemoteSettingsConfig {
        server_url: url.as_str().to_string(),
        collection_name: "doesn't matter".to_string(),
        bucket_name: "doesn't matter".to_string(),
    };

    let tmp_dir = TempDir::new("test_fs_client-test_simple")?;

    let aru = AvailableRandomizationUnits { client_id: None };
    let client = NimbusClient::new(Default::default(), tmp_dir.path(), config, aru)?;

    let experiments = client.get_all_experiments()?;
    assert_eq!(experiments.len(), 1);
    assert_eq!(experiments[0].slug, "secure-gold");
    // Once we can set the nimbus ID, we should set it to a uuid we know
    // gets enrolled.
    Ok(())
}
