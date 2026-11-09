/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::time::Duration;

/// Hard ceiling on any resolved TTL, regardless of source. Guards against a
/// misconfigured server (e.g. `Cache-Control: max-age=315360000`) pinning
/// responses in the cache for far longer than is reasonable.
pub(super) const MAX_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

// Deliberately unsorted: fields are in resolution priority order, matching
// `resolve()` below.
/// The TTL to use when storing a response in the cache, computed from
/// three possible sources.
///
/// `explicit` comes from the caller, `server_max_age` from the response's
/// `Cache-Control` header, and `default` from the cache's configuration.
pub(super) struct EffectiveTtl {
    /// Per-request override provided by the caller, if any.
    pub explicit: Option<Duration>,
    /// `Cache-Control: max-age` from the server response, if present.
    pub server_max_age: Option<Duration>,
    /// How old the response already was when received. Counts against
    /// `server_max_age` only.
    pub current_age: Duration,
    /// The cache's configured default TTL.
    pub default: Duration,
}

impl EffectiveTtl {
    /// Resolve the TTL by priority (highest to lowest):
    /// 1. `explicit` — caller-provided per-request override.
    /// 2. `server_max_age` — value of `Cache-Control: max-age` on the response,
    ///    minus `current_age`.
    /// 3. `default` — the cache's configured default.
    ///
    /// The result is capped at [`MAX_TTL`].
    pub(super) fn resolve(&self) -> Duration {
        let chosen = self
            .explicit
            .or(self
                .server_max_age
                .map(|max_age| max_age.saturating_sub(self.current_age)))
            .unwrap_or(self.default);
        chosen.min(MAX_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_overrides_server_max_age_and_default() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: Some(Duration::from_secs(60)),
            server_max_age: Some(Duration::from_secs(3600)),
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn falls_back_to_server_max_age_when_no_explicit() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: Some(Duration::from_secs(3600)),
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    fn falls_back_to_default_when_no_explicit_and_no_server_max_age() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: None,
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(300));
    }

    #[test]
    fn zero_server_max_age_yields_zero() {
        // Lets the strategy emit NoCache without a network round-trip.
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: Some(Duration::ZERO),
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, Duration::ZERO);
    }

    #[test]
    fn current_age_reduces_server_max_age() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: Some(Duration::from_secs(86400)),
            current_age: Duration::from_secs(72000),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(14400));
    }

    #[test]
    fn current_age_at_or_past_server_max_age_yields_zero() {
        for age in [3600, 7200] {
            let ttl = EffectiveTtl {
                default: Duration::from_secs(300),
                explicit: None,
                server_max_age: Some(Duration::from_secs(3600)),
                current_age: Duration::from_secs(age),
            }
            .resolve();
            assert_eq!(ttl, Duration::ZERO);
        }
    }

    #[test]
    fn current_age_does_not_reduce_explicit_ttl() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: Some(Duration::from_secs(60)),
            server_max_age: Some(Duration::from_secs(3600)),
            current_age: Duration::from_secs(7200),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn current_age_does_not_reduce_default_ttl() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: None,
            current_age: Duration::from_secs(7200),
        }
        .resolve();
        assert_eq!(ttl, Duration::from_secs(300));
    }

    #[test]
    fn server_max_age_is_capped_at_max_ttl() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: None,
            server_max_age: Some(Duration::from_secs(365 * 24 * 60 * 60)),
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn explicit_ttl_is_capped_at_max_ttl() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(300),
            explicit: Some(Duration::from_secs(30 * 24 * 60 * 60)),
            server_max_age: None,
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn default_ttl_is_capped_at_max_ttl() {
        let ttl = EffectiveTtl {
            default: Duration::from_secs(30 * 24 * 60 * 60),
            explicit: None,
            server_max_age: None,
            current_age: Duration::ZERO,
        }
        .resolve();
        assert_eq!(ttl, MAX_TTL);
    }
}
