/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::Row;
use std::time;

// from places
fn split_after_prefix(href: &str) -> (&str, &str) {
    match memchr::memchr(b':', href.as_bytes()) {
        None => ("", href),
        Some(index) => {
            let hb = href.as_bytes();
            let mut end = index + 1;
            if hb.len() >= end + 2 && hb[end] == b'/' && hb[end + 1] == b'/' {
                end += 2;
            }
            href.split_at(end)
        }
    }
}

/// Returns:
///
/// - the prefix (scheme, colon, and '//' if present)
/// - host:port
///
/// e.g. removes path, query, fragment, and userinfo.
pub fn prefix_hostport(href: &str) -> (&str, &str) {
    let (prefix, remainder) = split_after_prefix(href);

    let start = memchr::memchr(b'@', remainder.as_bytes())
        .map(|i| i + 1)
        .unwrap_or(0);

    let remainder = &remainder[start..];
    let end = memchr::memchr3(b'/', b'?', b'#', remainder.as_bytes()).unwrap_or(remainder.len());
    (prefix, &remainder[..end])
}

pub fn system_time_millis_from_row(row: &Row, col_name: &str) -> Result<time::SystemTime> {
    let time_ms = row
        .get_checked::<_, Option<i64>>(col_name)?
        .unwrap_or_default() as u64;
    Ok(time::UNIX_EPOCH + time::Duration::from_millis(time_ms))
}

pub fn duration_ms_i64(d: time::Duration) -> i64 {
    (d.as_secs() as i64) * 1000 + ((d.subsec_nanos() as i64) / 1_000_000)
}

pub fn system_time_ms_i64(t: time::SystemTime) -> i64 {
    duration_ms_i64(t.duration_since(time::UNIX_EPOCH).unwrap_or_default())
}

// Unfortunately, there's not a better way to turn on logging in tests AFAICT
#[cfg(test)]
pub(crate) fn init_test_logging() {
    use std::sync::{Once, ONCE_INIT};
    static INIT_LOGGING: Once = ONCE_INIT;
    INIT_LOGGING.call_once(|| {
        env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", "trace"));
    });
}
