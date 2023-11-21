/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use sha2::{Digest, Sha256};

#[derive(Default)]
pub(crate) struct Sha256Hasher {
    hasher: Sha256,
}

impl std::hash::Hasher for Sha256Hasher {
    fn finish(&self) -> u64 {
        let v = self.hasher.clone().finalize();
        u64::from_le_bytes(v[0..8].try_into().unwrap())
    }

    fn write(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }
}
