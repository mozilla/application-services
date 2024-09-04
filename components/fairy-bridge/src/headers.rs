/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::FairyBridgeError;
use std::borrow::Cow;

/// Normalize / validate a request header
///
/// This accepts both &str and String. It either returns the lowercase version or
/// `FairyBridgeError::InvalidRequestHeader`
pub fn normalize_request_header<'a>(name: impl Into<Cow<'a, str>>) -> crate::Result<String> {
    do_normalize_header(name).map_err(|name| FairyBridgeError::InvalidRequestHeader { name })
}

/// Normalize / validate a response header
///
/// This accepts both &str and String. It either returns the lowercase version or
/// `FairyBridgeError::InvalidRequestHeader`
pub fn normalize_response_header<'a>(name: impl Into<Cow<'a, str>>) -> crate::Result<String> {
    do_normalize_header(name).map_err(|name| FairyBridgeError::InvalidResponseHeader { name })
}

fn do_normalize_header<'a>(name: impl Into<Cow<'a, str>>) -> Result<String, String> {
    // Note: 0 = invalid, 1 = valid, 2 = valid but needs lowercasing. I'd use an
    // enum for this, but it would make this LUT *way* harder to look at. This
    // includes 0-9, a-z, A-Z (as 2), and ('!' | '#' | '$' | '%' | '&' | '\'' | '*'
    // | '+' | '-' | '.' | '^' | '_' | '`' | '|' | '~'), matching the field-name
    // token production defined at https://tools.ietf.org/html/rfc7230#section-3.2.
    static VALID_HEADER_LUT: [u8; 256] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0,
        0, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
        2, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    let mut name = name.into();

    if name.len() == 0 {
        return Err(name.to_string());
    }
    let mut need_lower_case = false;
    for b in name.bytes() {
        let validity = VALID_HEADER_LUT[b as usize];
        if validity == 0 {
            return Err(name.to_string());
        }
        if validity == 2 {
            need_lower_case = true;
        }
    }
    if need_lower_case {
        // Only do this if needed, since it causes us to own the header.
        name.to_mut().make_ascii_lowercase();
    }
    Ok(name.to_string())
}

// Default headers for easy usage
pub const ACCEPT_ENCODING: &str = "accept-encoding";
pub const ACCEPT: &str = "accept";
pub const AUTHORIZATION: &str = "authorization";
pub const CONTENT_TYPE: &str = "content-type";
pub const ETAG: &str = "etag";
pub const IF_NONE_MATCH: &str = "if-none-match";
pub const USER_AGENT: &str = "user-agent";
// non-standard, but it's convenient to have these.
pub const RETRY_AFTER: &str = "retry-after";
pub const X_IF_UNMODIFIED_SINCE: &str = "x-if-unmodified-since";
pub const X_KEYID: &str = "x-keyid";
pub const X_LAST_MODIFIED: &str = "x-last-modified";
pub const X_TIMESTAMP: &str = "x-timestamp";
pub const X_WEAVE_NEXT_OFFSET: &str = "x-weave-next-offset";
pub const X_WEAVE_RECORDS: &str = "x-weave-records";
pub const X_WEAVE_TIMESTAMP: &str = "x-weave-timestamp";
pub const X_WEAVE_BACKOFF: &str = "x-weave-backoff";
