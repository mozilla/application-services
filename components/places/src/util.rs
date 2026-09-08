/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::{Error, Result};
use std::path::Path;
use url::Url;

/// Equivalent to `&s[..max_len.min(s.len())]`, but handles the case where
/// `s.is_char_boundary(max_len)` is false (which would otherwise panic).
pub fn slice_up_to(s: &str, max_len: usize) -> &str {
    if max_len >= s.len() {
        return s;
    }
    let mut idx = max_len;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

/// If `p` is a file URL, return it, otherwise try and make it one.
///
/// Errors if `p` is a relative non-url path, or if it's a URL path
/// that's isn't a `file:` URL.
pub fn ensure_url_path(p: impl AsRef<Path>) -> Result<Url> {
    if let Some(u) = p.as_ref().to_str().and_then(|s| Url::parse(s).ok()) {
        if u.scheme() == "file" {
            Ok(u)
        } else {
            Err(Error::IllegalDatabasePath(p.as_ref().to_owned()))
        }
    } else {
        let p = p.as_ref();
        let u = Url::from_file_path(p).map_err(|_| Error::IllegalDatabasePath(p.to_owned()))?;
        Ok(u)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_slice_up_to() {
        assert_eq!(slice_up_to("abcde", 4), "abcd");
        assert_eq!(slice_up_to("abcde", 5), "abcde");
        assert_eq!(slice_up_to("abcde", 6), "abcde");
        let s = "abcd😀";
        assert_eq!(s.len(), 8);
        assert_eq!(slice_up_to(s, 4), "abcd");
        assert_eq!(slice_up_to(s, 5), "abcd");
        assert_eq!(slice_up_to(s, 6), "abcd");
        assert_eq!(slice_up_to(s, 7), "abcd");
        assert_eq!(slice_up_to(s, 8), s);
    }
    #[test]
    fn test_ensure_url() {
        assert_eq!(
            ensure_url_path("file:///foo%20bar/baz").unwrap().as_str(),
            "file:///foo%20bar/baz"
        );

        assert_eq!(
            ensure_url_path("/foo bar/baz").unwrap().as_str(),
            "file:///foo%20bar/baz"
        );

        assert!(ensure_url_path("bar").is_err());

        assert!(ensure_url_path("http://www.not-a-file.com").is_err());
    }
}
