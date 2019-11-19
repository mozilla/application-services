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
///
/// Essentially equivalent to explicitly writing `Err(e)?`, but logs the error,
/// and is more well-behaved from a type-checking perspective.
macro_rules! throw {
    ($e:expr $(,)?) => {{
        let e = $e;
        log::error!("Error: {}", e);
        return Err(std::convert::Into::into(e));
    }};
}

/// Like assert! but with `throw!` and not `panic!`.
///
/// Equivalent to explicitly writing `if !cond { throw!(e) }`, but logs what the
/// failed condition was (at warning levels).
macro_rules! ensure {
    ($cond:expr, $e:expr $(,)?) => {
        if !($cond) {
            log::warn!(concat!("Ensure ", stringify!($cond), " failed!"));
            throw!($e)
        }
    };
}
