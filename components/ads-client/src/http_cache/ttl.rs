/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::time::Duration;

/// The TTL to use when storing a response in the cache, computed from
/// three possible sources.
///
/// `explicit` comes from the caller, `server_max_age` from the response's
/// `Cache-Control` header, and `default` from the cache's configuration.
pub struct EffectiveTtl {
    /// Per-request override provided by the caller, if any.
    pub explicit: Option<Duration>,
    /// `Cache-Control: max-age` from the server response, if present.
    pub server_max_age: Option<Duration>,
    /// The cache's configured default TTL.
    pub default: Duration,
}

impl EffectiveTtl {
    /// Resolve the TTL by priority (highest to lowest):
    /// 1. `explicit` — caller-provided per-request override.
    /// 2. `server_max_age` — value of `Cache-Control: max-age` on the response.
    /// 3. `default` — the cache's configured default.
    pub fn resolve(&self) -> Duration {
        self.explicit
            .or(self.server_max_age)
            .unwrap_or(self.default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_overrides_server_max_age_and_default() {
        let ttl = EffectiveTtl {
            explicit: Some(Duration::from_secs(60)),
            server_max_age: Some(Duration::from_secs(3600)),
            default: Duration::from_secs(300),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn falls_back_to_server_max_age_when_no_explicit() {
        let ttl = EffectiveTtl {
            explicit: None,
            server_max_age: Some(Duration::from_secs(3600)),
            default: Duration::from_secs(300),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    fn falls_back_to_default_when_no_explicit_and_no_server_max_age() {
        let ttl = EffectiveTtl {
            explicit: None,
            server_max_age: None,
            default: Duration::from_secs(300),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(300));
    }

    #[test]
    fn zero_server_max_age_yields_zero() {
        // Lets the strategy emit NoCache without a network round-trip.
        let ttl = EffectiveTtl {
            explicit: None,
            server_max_age: Some(Duration::ZERO),
            default: Duration::from_secs(300),
        }
        .resolve();
        assert_eq!(ttl, Duration::ZERO);
    }
}
