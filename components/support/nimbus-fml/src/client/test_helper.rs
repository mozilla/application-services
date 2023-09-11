/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::Result, intermediate_representation::FeatureManifest, FmlClient};

pub(crate) fn client(path: &str, channel: &str) -> Result<FmlClient> {
    let root = env!("CARGO_MANIFEST_DIR");
    let fixtures = format!("{root}/fixtures/fe");
    FmlClient::new_with_ref(
        format!("@my/fixtures/{path}"),
        channel.to_string(),
        Some(fixtures),
    )
}

impl From<FeatureManifest> for FmlClient {
    fn from(manifest: FeatureManifest) -> Self {
        Self::new_from_manifest(manifest)
    }
}
