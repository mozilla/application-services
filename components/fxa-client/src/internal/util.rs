/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{Error, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rc_crypto::rand;
use std::time::{SystemTime, UNIX_EPOCH};

// Gets the unix epoch in ms.
pub fn now() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Something is very wrong.");
    since_epoch.as_secs() * 1000 + u64::from(since_epoch.subsec_nanos()) / 1_000_000
}

pub fn now_secs() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Something is very wrong.");
    since_epoch.as_secs()
}

/// Gets unix timestamp at `days` days ago
pub fn past_timestamp(days: u64) -> u64 {
    // 1000 milliseconds, 60 seconds, 60 minutes, 24 hours
    now() - (1000 * 60 * 60 * 24 * days)
}

pub fn random_base64_url_string(len: usize) -> Result<String> {
    let mut out = vec![0u8; len];
    rand::fill(&mut out)?;
    Ok(URL_SAFE_NO_PAD.encode(&out))
}

pub trait Xorable {
    #[allow(dead_code)]
    fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>>;
}

impl Xorable for [u8] {
    fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>> {
        if self.len() != other.len() {
            Err(Error::XorLengthMismatch(self.len(), other.len()))
        } else {
            Ok(self
                .iter()
                .zip(other.iter())
                .map(|(&x, &y)| x ^ y)
                .collect())
        }
    }
}
