use std::time::{SystemTime, UNIX_EPOCH};

// Gets the unix epoch in ms.
pub fn now() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Something is very wrong.");
    since_epoch.as_secs() * 1000 + since_epoch.subsec_nanos() as u64 / 1_000_000
}

pub fn now_secs() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Something is very wrong.");
    since_epoch.as_secs()
}

pub trait Xorable {
    fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>, &'static str>;
}

impl Xorable for [u8] {
    fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>, &'static str> {
        if self.len() != other.len() {
            Err("Slices have different sizes.")
        } else {
            Ok(self.iter()
                .zip(other.iter())
                .map(|(&x, &y)| x ^ y)
                .collect())
        }
    }
}
