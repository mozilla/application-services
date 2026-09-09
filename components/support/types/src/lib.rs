/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as RusqliteResult;
use serde_derive::*;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Typesafe way to manage timestamps.
// We should probably work out how to share this too?
#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize, Default,
)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        SystemTime::now().into()
    }

    /// Returns None if `other` is later than `self` (Duration may not represent
    /// negative timespans in rust).
    #[inline]
    pub fn duration_since(self, other: Timestamp) -> Option<Duration> {
        // just do this via SystemTime.
        SystemTime::from(self).duration_since(other.into()).ok()
    }

    #[inline]
    pub fn checked_sub(self, d: Duration) -> Option<Timestamp> {
        SystemTime::from(self).checked_sub(d).map(Timestamp::from)
    }

    #[inline]
    pub fn checked_add(self, d: Duration) -> Option<Timestamp> {
        SystemTime::from(self).checked_add(d).map(Timestamp::from)
    }

    pub fn as_millis(self) -> u64 {
        self.0
    }

    pub fn as_millis_i64(self) -> i64 {
        self.0 as i64
    }
    /// In desktop sync, bookmarks are clamped to Jan 23, 1993 (which is 727747200000)
    /// There's no good reason history records could be older than that, so we do
    /// the same here (even though desktop's history currently doesn't)
    /// XXX - there's probably a case to be made for this being, say, 5 years ago -
    /// then all requests earlier than that are collapsed into a single visit at
    /// this timestamp.
    pub const EARLIEST: Timestamp = Timestamp(727_747_200_000);

    /// [`sanitize_timestamp`] for an already-constructed `Timestamp`.
    ///
    /// The value goes through `i64` on the way, so a negative millisecond count
    /// that was reinterpreted as a huge `u64` is recovered and reported as 0
    /// rather than surviving as an instant 580 million years hence.
    pub fn sanitized(self) -> Timestamp {
        Timestamp(sanitize_timestamp(self.0 as i64) as u64)
    }
}

/// The largest instant a JS `Date` can represent, and so the largest a timestamp
/// crossing our FFI may be: 100,000,000 days either side of the epoch, see
/// MAX_DATE_MS in
/// <https://searchfox.org/firefox-main/source/toolkit/components/passwordmgr/LoginManager.sys.mjs>
pub const MAX_DATE_MS: i64 = 8_640_000_000_000_000;

/// Coerce a timestamp from an untrusted source into an instant our consumers can
/// represent.
///
/// Anything outside `[0, MAX_DATE_MS]` is reported as 0, "we don't know when". We
/// repair rather than reject because these values arrive from places we cannot
/// refuse - the local database, the sync server, and metadata an application
/// hands us on import - and a single corrupt record must not make the whole
/// store unreadable.
pub fn sanitize_timestamp(time_ms: i64) -> i64 {
    if (0..=MAX_DATE_MS).contains(&time_ms) {
        time_ms
    } else {
        0
    }
}

impl From<Timestamp> for u64 {
    #[inline]
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

impl From<SystemTime> for Timestamp {
    #[inline]
    fn from(st: SystemTime) -> Self {
        let d = st.duration_since(UNIX_EPOCH).unwrap(); // hrmph - unwrap doesn't seem ideal
        Timestamp((d.as_secs()) * 1000 + (u64::from(d.subsec_nanos()) / 1_000_000))
    }
}

impl From<Timestamp> for SystemTime {
    #[inline]
    fn from(ts: Timestamp) -> Self {
        UNIX_EPOCH + Duration::from_millis(ts.into())
    }
}

impl From<u64> for Timestamp {
    #[inline]
    fn from(ts: u64) -> Self {
        assert!(ts != 0);
        Timestamp(ts)
    }
}

impl fmt::Display for Timestamp {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for Timestamp {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0 as i64)) // hrm - no u64 in rusqlite
    }
}

impl FromSql for Timestamp {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_i64().map(|v| Timestamp(v as u64)) // hrm - no u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `timeCreated` of this shape seen in telemetry. Each is `double(2^64 - d) / 1000`
    /// for a small positive `d`, and each is divisible by 4 - which at this magnitude, where
    /// the f64 spacing is exactly 4, means each is exactly representable. That is the evidence
    /// the division happened in JS floating point rather than in integer arithmetic.
    const OBSERVED_CORRUPT: [i64; 9] = [
        18446744071619076,
        18446744071857664, // the value reported in bug 2066257
        18446744071965092,
        18446744072105044,
        18446744072410028,
        18446744072560092,
        18446744072924040,
        18446744073032264,
        18446744073217880,
    ];

    /// The set of values a negative microsecond timestamp can become when it is reinterpreted
    /// as a u64 and then divided by 1000.
    fn u64_wrap_family() -> std::ops::RangeInclusive<i64> {
        let lo = ((1u128 << 64) - (1u128 << 63)) / 1000;
        let hi = ((1u128 << 64) - 1) / 1000;
        (lo as i64)..=(hi as i64)
    }

    #[test]
    fn test_sanitize_timestamp_preserves_valid_instants() {
        let now_ms = Timestamp::now().as_millis_i64();
        assert_eq!(sanitize_timestamp(0), 0);
        assert_eq!(sanitize_timestamp(1), 1);
        assert_eq!(sanitize_timestamp(now_ms), now_ms);
        // Implausible but representable instants are left alone on purpose - deciding that
        // they are wrong needs a clock we can trust, and the local one is not it.
        assert_eq!(sanitize_timestamp(now_ms + 1000), now_ms + 1000);
        assert_eq!(sanitize_timestamp(MAX_DATE_MS), MAX_DATE_MS);
    }

    #[test]
    fn test_sanitize_timestamp_reports_invalid_as_unknown() {
        // The largest integer an f64 carries exactly. Our bound is tighter, so a value the
        // FFI could technically represent is still repaired when it is not a date.
        const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

        assert_eq!(sanitize_timestamp(-1), 0);
        assert_eq!(sanitize_timestamp(i64::MIN), 0);
        assert_eq!(sanitize_timestamp(MAX_DATE_MS + 1), 0);
        assert_eq!(sanitize_timestamp(MAX_SAFE_INTEGER), 0);
        assert_eq!(sanitize_timestamp(i64::MAX), 0);
    }

    /// The values actually observed in the wild, rather than ones we invented.
    #[test]
    fn test_sanitize_timestamp_repairs_every_observed_value() {
        for time_ms in OBSERVED_CORRUPT {
            assert_eq!(sanitize_timestamp(time_ms), 0, "{time_ms} survived");
        }
    }

    /// Why 0 is the right answer and not merely a safe one: every u64-reinterpreted negative
    /// timestamp lies above the bound, so the test catches the entire family, and every member
    /// of it unwraps to an instant before the epoch.
    #[test]
    fn test_u64_wrap_family_is_caught_and_unwraps_before_the_epoch() {
        let family = u64_wrap_family();
        assert!(
            *family.start() > MAX_DATE_MS,
            "family starts at {}, which the bound would miss",
            family.start()
        );

        // Recovering the original means subtracting 2^64/1000; do it in i128 so the
        // intermediate cannot overflow.
        let wrap_offset = (1i128 << 64) / 1000;
        for time_ms in OBSERVED_CORRUPT
            .into_iter()
            .chain([*family.start(), *family.end()])
        {
            assert!(family.contains(&time_ms), "{time_ms} is not of this shape");
            assert_eq!(sanitize_timestamp(time_ms), 0);
            assert!(
                time_ms as i128 - wrap_offset <= 0,
                "{time_ms} unwraps to a positive instant"
            );
        }
    }

    /// `Timestamp` holds a `u64`, so the recovery has to survive the round trip: a
    /// negative millisecond count stored as a huge unsigned value must come back as 0.
    #[test]
    fn test_timestamp_sanitized_recovers_u64_reinterpretation() {
        assert_eq!(Timestamp(0).sanitized(), Timestamp(0));
        assert_eq!(Timestamp(1).sanitized(), Timestamp(1));
        let now = Timestamp::now();
        assert_eq!(now.sanitized(), now);
        assert_eq!(
            Timestamp(MAX_DATE_MS as u64).sanitized(),
            Timestamp(MAX_DATE_MS as u64)
        );

        // -1 as it appears once stored in and read back out of a u64.
        assert_eq!(Timestamp(u64::MAX).sanitized(), Timestamp(0));
        assert_eq!(Timestamp(18446744071857664).sanitized(), Timestamp(0));
        assert_eq!(Timestamp(MAX_DATE_MS as u64 + 1).sanitized(), Timestamp(0));
    }
}
