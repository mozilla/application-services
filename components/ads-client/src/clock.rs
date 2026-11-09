/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use chrono::{DateTime, Utc};
#[cfg(test)]
use std::{sync::Arc, time::Duration};

/// Where the current time comes from. Outside tests it is always the system
/// clock: the mock only exists in test builds.
#[derive(Clone, Default)]
pub(crate) struct Clock(Source);

#[derive(Clone, Default)]
enum Source {
    #[default]
    System,
    #[cfg(test)]
    Mock(Arc<MockClock>),
}

impl Clock {
    pub(crate) fn now(&self) -> DateTime<Utc> {
        match &self.0 {
            Source::System => Utc::now(),
            #[cfg(test)]
            Source::Mock(mock) => *mock.now.lock(),
        }
    }

    /// A clock that stands still at the current second until the returned
    /// handle moves it.
    #[cfg(test)]
    pub(crate) fn mock() -> (Self, Arc<MockClock>) {
        use chrono::SubsecRound;

        let mock = Arc::new(MockClock {
            now: parking_lot::Mutex::new(Utc::now().trunc_subsecs(0)),
        });
        (Self(Source::Mock(Arc::clone(&mock))), mock)
    }
}

#[cfg(test)]
pub(crate) struct MockClock {
    now: parking_lot::Mutex<DateTime<Utc>>,
}

#[cfg(test)]
impl MockClock {
    pub(crate) fn advance(&self, by: Duration) {
        *self.now.lock() += by;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_moves_only_when_advanced() {
        let (clock, mock) = Clock::mock();
        let start = clock.now();
        assert_eq!(clock.now(), start);

        mock.advance(Duration::from_secs(90));
        assert_eq!(clock.now(), start + Duration::from_secs(90));
    }

    #[test]
    fn clones_share_the_mock() {
        let (clock, mock) = Clock::mock();
        let copy = clock.clone();
        mock.advance(Duration::from_secs(5));
        assert_eq!(copy.now(), clock.now());
    }
}
