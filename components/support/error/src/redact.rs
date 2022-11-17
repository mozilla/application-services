/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Functions to redact strings to remove PII before logging them

/// Redact a URL, replacing all characters other than [`:`, `/`] with `x`
pub fn redact_url(url: &str) -> String {
    url.replace(|ch| ch != ':' && ch != '/', "x")
}

/// Redact compact jwe string (Five base64 segments, separated by `.` chars)
pub fn redact_compact_jwe(url: &str) -> String {
    url.replace(|ch| ch != '.', "x")
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_redact_url() {
        assert_eq!(
            redact_url("http://some.website.com/index.html"),
            "xxxx://xxxxxxxxxxxxxxxx/xxxxxxxxxx"
        );
        assert_eq!(
            redact_url("http://some.website.com:8000/foo/bar/baz"),
            "xxxx://xxxxxxxxxxxxxxxx:xxxx/xxx/xxx/xxx"
        );
    }

    #[test]
    fn test_redact_compact_jwe() {
        assert_eq!(redact_compact_jwe("abc.1234.x3243"), "xxx.xxxx.xxxxx")
    }
}
