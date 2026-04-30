/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::time::Duration;

use super::MAX_TTL;

/// Inputs to [`resolve_ttl`].
///
/// Each field comes from a different source: `explicit` from the caller,
/// `server_max_age` from the response's `Cache-Control` header, and
/// `default` from the cache's configuration.
pub(super) struct TtlInputs {
    /// Per-request override provided by the caller, if any.
    pub explicit: Option<Duration>,
    /// `Cache-Control: max-age` from the server response, if present.
    pub server_max_age: Option<Duration>,
    /// The cache's configured default TTL.
    pub default: Duration,
}

/// Resolve the TTL to use when storing a response in the cache.
///
/// Priority (highest to lowest):
/// 1. `explicit` — caller-provided per-request override.
/// 2. `server_max_age` — value of `Cache-Control: max-age` on the response.
/// 3. `default` — the cache's configured default.
///
/// The resulting TTL is capped at [`MAX_TTL`] for safety.
pub(super) fn resolve_ttl(inputs: TtlInputs) -> Duration {
    let chosen = inputs
        .explicit
        .or(inputs.server_max_age)
        .unwrap_or(inputs.default);
    std::cmp::min(chosen, MAX_TTL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_overrides_server_max_age_and_default() {
        let ttl = resolve_ttl(TtlInputs {
            explicit: Some(Duration::from_secs(60)),
            server_max_age: Some(Duration::from_secs(3600)),
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn falls_back_to_server_max_age_when_no_explicit() {
        let ttl = resolve_ttl(TtlInputs {
            explicit: None,
            server_max_age: Some(Duration::from_secs(3600)),
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    fn falls_back_to_default_when_no_explicit_and_no_server_max_age() {
        let ttl = resolve_ttl(TtlInputs {
            explicit: None,
            server_max_age: None,
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, Duration::from_secs(300));
    }

    #[test]
    fn caps_server_max_age_at_max_ttl() {
        let ttl = resolve_ttl(TtlInputs {
            explicit: None,
            // 30 days, well over MAX_TTL (7 days)
            server_max_age: Some(Duration::from_secs(60 * 60 * 24 * 30)),
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn caps_explicit_at_max_ttl() {
        let ttl = resolve_ttl(TtlInputs {
            explicit: Some(Duration::from_secs(60 * 60 * 24 * 30)),
            server_max_age: None,
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn zero_server_max_age_yields_zero() {
        // Lets the strategy emit NoCache without a network round-trip.
        let ttl = resolve_ttl(TtlInputs {
            explicit: None,
            server_max_age: Some(Duration::ZERO),
            default: Duration::from_secs(300),
        });
        assert_eq!(ttl, Duration::ZERO);
    }
}
