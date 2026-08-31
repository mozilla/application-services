/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::Row;
use std::time;
use url::Url;

pub fn url_host_port(url_str: &str) -> Option<String> {
    let url = Url::parse(url_str).ok()?;
    let host = url.host_str()?;
    Some(if let Some(p) = url.port() {
        format!("{}:{}", host, p)
    } else {
        host.to_string()
    })
}

pub fn system_time_millis_from_row(row: &Row<'_>, col_name: &str) -> Result<time::SystemTime> {
    let time_ms = sanitize_timestamp(row.get::<_, Option<i64>>(col_name)?.unwrap_or_default());
    // `sanitize_timestamp` guarantees a non-negative value, so this cast is lossless.
    Ok(time::UNIX_EPOCH + time::Duration::from_millis(time_ms as u64))
}

/// The largest instant a JS `Date` can represent, and so the largest a logins timestamp may
/// be: 100,000,000 days either side of the epoch, see MAX_DATE_MS in
/// https://searchfox.org/firefox-main/source/toolkit/components/passwordmgr/LoginManager.sys.mjs
pub const MAX_DATE_MS: i64 = 8_640_000_000_000_000;

/// Coerce a timestamp from an untrusted source into an instant our consumers can represent.
///
/// Anything outside `[0, MAX_DATE_MS]` is reported as 0, "we don't know when". We repair
/// rather than reject because these values arrive from places we cannot refuse, the local
/// database and the sync server, and a single corrupt record must not make the whole store
/// unreadable.
pub fn sanitize_timestamp(time_ms: i64) -> i64 {
    if (0..=MAX_DATE_MS).contains(&time_ms) {
        time_ms
    } else {
        0
    }
}

pub fn duration_ms_i64(d: time::Duration) -> i64 {
    (d.as_secs() as i64) * 1000 + (i64::from(d.subsec_nanos()) / 1_000_000)
}

pub fn system_time_ms_i64(t: time::SystemTime) -> i64 {
    duration_ms_i64(t.duration_since(time::UNIX_EPOCH).unwrap_or_default())
}

#[cfg(test)]
pub(crate) fn init_test_logging() {
    use std::sync::Once;
    static INIT_LOGGING: Once = Once::new();
    INIT_LOGGING.call_once(|| {
        error_support::init_for_tests_with_level(error_support::Level::Trace);
    });
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
        let now_ms = system_time_ms_i64(time::SystemTime::now());
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

    #[test]
    fn test_system_time_millis_from_row_survives_negative() {
        // Reading a negative timestamp used to reinterpret it as a u64, producing a duration
        // of some 580 million years.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (local_modified INTEGER); INSERT INTO t VALUES (-1);")
            .unwrap();
        let time = conn
            .query_row("SELECT local_modified FROM t", [], |row| {
                Ok(system_time_millis_from_row(row, "local_modified"))
            })
            .unwrap()
            .unwrap();
        assert_eq!(time, time::UNIX_EPOCH);
    }
}
