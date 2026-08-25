/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::clock::Clock;

pub struct ImpressionLogClock;

impl Clock for ImpressionLogClock {
    fn now_epoch_seconds(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }

    #[cfg(test)]
    fn advance(&self, _secs: i64) {
        panic!(
            "You cannot advance a non-test clock.
            Be sure to build the log or store with the test clock for time-dependent tests."
        )
    }
}
