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

// The bound and the repair itself live next to `Timestamp` in the `types` crate, so
// logins and autofill cannot drift on what a representable date is.
pub use types::sanitize_timestamp;

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
