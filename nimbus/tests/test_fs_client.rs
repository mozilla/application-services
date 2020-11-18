/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Simple tests for our file-system client

#[cfg(feature = "rkv-safe-mode")]
use nimbus::error::Result;

// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.
#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_simple() -> Result<()> {
    use nimbus::{NimbusClient, RemoteSettingsConfig};
    use std::path::PathBuf;
    use tempdir::TempDir;
    use url::Url;

    let _ = env_logger::try_init();

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("tests/experiments");

    let url = Url::from_file_path(dir).expect("experiments dir should exist");

    let config = RemoteSettingsConfig {
        server_url: url.as_str().to_string(),
        bucket_name: "doesn't matter".to_string(),
        collection_name: "doesn't matter".to_string(),
    };

    let tmp_dir = TempDir::new("test_fs_client-test_simple")?;

    let aru = Default::default();
    let client = NimbusClient::new(Default::default(), tmp_dir.path(), Some(config), aru)?;
    client.update_experiments()?;

    let experiments = client.get_all_experiments()?;
    assert_eq!(experiments.len(), 1);
    assert_eq!(experiments[0].slug, "secure-gold");
    // Once we can set the nimbus ID, we should set it to a uuid we know
    // gets enrolled.
    Ok(())
}
