/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub trait Clock: Send + Sync + 'static {
    fn now_epoch_seconds(&self) -> i64;
    #[cfg(test)]
    fn advance(&self, secs: i64);
}

#[cfg(test)]
pub struct TestClock {
    now: std::sync::atomic::AtomicI64,
}

#[cfg(test)]
impl TestClock {
    pub fn new(start: i64) -> Self {
        Self {
            now: std::sync::atomic::AtomicI64::new(start),
        }
    }
}

#[cfg(test)]
impl Clock for TestClock {
    fn now_epoch_seconds(&self) -> i64 {
        self.now.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn advance(&self, secs: i64) {
        self.now
            .fetch_add(secs, std::sync::atomic::Ordering::Relaxed);
    }
}
