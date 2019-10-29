/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// For use with `#[serde(skip_serializing_if = )]`
#[inline]
pub fn is_default<T: PartialEq + Default>(v: &T) -> bool {
    *v == T::default()
}

/// Returns true if the byte `b` is a valid base64url byte.
#[inline]
#[rustfmt::skip]
pub fn is_base64url_byte(b: u8) -> bool {
    // For some reason, if this is indented the way rustfmt wants,
    // the next time this file is opened, VSCode deduces it *must*
    // actually use 8 space indent, and converts the whole file on
    // save. This is a VSCode bug, but is really annoying, so I'm
    // just preventing rustfmt from reformatting this to avoid it.
    (b'A' <= b && b <= b'Z') ||
    (b'a' <= b && b <= b'z') ||
    (b'0' <= b && b <= b'9') ||
    b == b'-' ||
    b == b'_'
}

/// Return with the provided Err(error) after invoking Into conversions
#[macro_export]
macro_rules! throw {
    ($e:expr) => {{
        log::error!("Error: {}", $e);
        return Err(::std::convert::Into::into($e));
    }};
}

/// Like assert! but with `throw!` and not `panic!`.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            log::warn!(concat!("Ensure ", stringify!($cond), " failed!"));
            throw!($e)
        }
    };
}

/// Release of WorldWideWeb, the first web browser. Synced data could never come
/// from before this date. XXX this could be untrue for new collections...
pub const EARLIEST_SANE_TIME: i64 = 662_083_200_000;
