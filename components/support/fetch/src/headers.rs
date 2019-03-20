/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use std::iter::FromIterator;
use std::str::FromStr;

pub use name::{HeaderName, InvalidHeaderName};
mod name;

/// A single header. Typically you will not interact with this directly.
#[derive(Clone, Debug, PartialEq, PartialOrd, Hash, Eq, Ord)]
pub struct Header {
    pub name: HeaderName,
    pub value: String,
}

/// A list of headers.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Headers {
    headers: Vec<Header>,
}

impl Headers {
    /// Initialize an empty list of headers.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Initialize an empty list of headers backed by a vector with the provided
    /// capacity.
    pub fn with_capacity(c: usize) -> Self {
        Self {
            headers: Vec::with_capacity(c),
        }
    }

    /// Convert this list of headers to a Vec<Header>
    #[inline]
    pub fn into_vec(self) -> Vec<Header> {
        self.headers
    }

    /// Returns the number of headers.
    #[inline]
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Returns true if `len()` is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }
    /// Clear this set of headers.
    #[inline]
    pub fn clear(&mut self) {
        self.headers.clear();
    }

    /// Insert or update a new header.
    ///
    /// ## Example
    /// ```
    /// # use support_fetch::Headers;
    /// let mut h = Headers::new();
    /// h.insert("My-Cool-Header", "example");
    /// assert_eq!(h.get("My-Cool-Header"), Some("example"));
    ///
    /// // Note: names are sensitive
    /// assert_eq!(h.get("my-cool-header"), Some("example"));
    ///
    /// // Also note, constants for headers are in `support_fetch::header_names`, and
    /// // you can chain the result of this function.
    /// h.insert(support_fetch::header_names::CONTENT_TYPE, "something...")
    ///  .insert("Something-Else", "etc");
    /// ```
    pub fn insert<N, V>(&mut self, name: N, value: V) -> &mut Self
    where
        N: Into<HeaderName> + PartialEq<HeaderName>,
        V: Into<String>,
    {
        if let Some(entry) = self.headers.iter_mut().find(|h| name == h.name) {
            entry.value = value.into();
        } else {
            self.headers.push(Header {
                name: name.into(),
                value: value.into(),
            });
        }
        self
    }

    /// Insert the provided header unless a header is already specified.
    /// Mostly used internally, e.g. to set "Content-Type: application/json"
    /// in `Request::json()` unless it has been set specifically.
    pub fn insert_or_ignore<N, V>(&mut self, name: N, value: V) -> &mut Self
    where
        N: Into<HeaderName> + PartialEq<HeaderName>,
        V: Into<String>,
    {
        if !self.headers.iter_mut().any(|h| name == h.name) {
            self.headers.push(Header {
                name: name.into(),
                value: value.into(),
            });
        }
        self
    }

    /// Insert or update a header directly. Typically you will want to use
    /// `insert` over this, as it performs less work if the header needs
    /// updating instead of insertion.
    pub fn insert_header(&mut self, new: Header) -> &mut Self {
        if let Some(entry) = self.headers.iter_mut().find(|h| h.name == new.name) {
            entry.value = new.value;
        } else {
            self.headers.push(new);
        }
        self
    }

    /// Add all the headers in the provided iterator to this list of headers.
    pub fn extend<I>(&mut self, iter: I) -> &mut Self
    where
        I: IntoIterator<Item = Header>,
    {
        let it = iter.into_iter();
        self.headers.reserve(it.size_hint().0);
        for h in it {
            self.insert_header(h);
        }
        self
    }

    /// Get the header object with the requested name. Usually, you will
    /// want to use `get()` or `get_as::<T>()` instead.
    pub fn get_header<S>(&self, name: S) -> Option<&Header>
    where
        S: PartialEq<HeaderName>,
    {
        self.headers.iter().find(|h| name == h.name)
    }

    /// Get the value of the header with the provided name.
    ///
    /// See also `get_as`.
    ///
    /// ## Example
    /// ```
    /// # use support_fetch::{Headers, header_names::CONTENT_TYPE};
    /// let mut h = Headers::new();
    /// h.insert(CONTENT_TYPE, "application/json");
    /// assert_eq!(h.get(CONTENT_TYPE), Some("application/json"));
    /// assert_eq!(h.get("Something-Else"), None);
    /// ```
    pub fn get<S>(&self, name: S) -> Option<&str>
    where
        S: PartialEq<HeaderName>,
    {
        self.get_header(name).map(|h| h.value.as_str())
    }

    /// Get the value of the header with the provided name, and
    /// attempt to parse it using [`std::str::FromStr`].
    ///
    /// - If the header is missing, it returns None.
    /// - If the header is present but parsing failed, returns
    ///   `Some(Err(<error returned by parsing>))`.
    /// - Otherwise, returns `Some(Ok(result))`.
    ///
    /// Note that if `Option<Result<T, E>>` is inconvenient for you,
    /// and you wish this returned `Result<Option<T>, E>`, you may use
    /// the built-in `transpose()` method to convert between them.
    ///
    /// ```
    /// # use support_fetch::Headers;
    /// let mut h = Headers::new();
    /// h.insert("Example", "1234").insert("Illegal", "abcd");
    /// let v: Option<Result<i64, _>> = h.get_as("Example");
    /// assert_eq!(v, Some(Ok(1234)));
    /// assert_eq!(h.get_as::<i64, _>("Example"), Some(Ok(1234)));
    /// assert_eq!(h.get_as::<i64, _>("Illegal"), Some("abcd".parse::<i64>()));
    /// assert_eq!(h.get_as::<i64, _>("Something-Else"), None);
    /// ```
    pub fn get_as<T, S>(&self, name: S) -> Option<Result<T, <T as FromStr>::Err>>
    where
        T: FromStr,
        S: PartialEq<HeaderName>,
    {
        self.get(name).map(|val| val.parse::<T>())
    }

    /// Get the value of the header with the provided name, and
    /// attempt to parse it using [`std::str::FromStr`].
    ///
    /// This is a variant of `get_as` that returns None on error,
    /// intended to be used for cases where missing and invalid
    /// headers should be treated the same. (With `get_as` this
    /// requires `h.get_as(...).and_then(|r| r.ok())`, which is
    /// somewhat opaque.
    pub fn try_get<T, S>(&self, name: S) -> Option<T>
    where
        T: FromStr,
        S: PartialEq<HeaderName>,
    {
        self.get(name).and_then(|val| val.parse::<T>().ok())
    }

    /// Get an iterator over the headers in no particular order.
    ///
    /// Note that we also implement IntoIterator.
    pub fn iter(&self) -> <&Headers as IntoIterator>::IntoIter {
        self.into_iter()
    }
}

impl std::iter::IntoIterator for Headers {
    type IntoIter = <Vec<Header> as IntoIterator>::IntoIter;
    type Item = Header;
    fn into_iter(self) -> Self::IntoIter {
        self.headers.into_iter()
    }
}

impl<'a> std::iter::IntoIterator for &'a Headers {
    type IntoIter = <&'a [Header] as IntoIterator>::IntoIter;
    type Item = &'a Header;
    fn into_iter(self) -> Self::IntoIter {
        (&self.headers[..]).iter()
    }
}

impl FromIterator<Header> for Headers {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Header>,
    {
        let mut v = iter.into_iter().collect::<Vec<Header>>();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v.reverse();
        v.dedup_by(|a, b| a.name == b.name);
        Headers { headers: v }
    }
}

pub mod consts {
    use super::name::HeaderName;
    macro_rules! def_header_consts {
        ($(($NAME:ident, $string:literal)),* $(,)?) => {
            $(pub const $NAME: HeaderName = HeaderName(std::borrow::Cow::Borrowed($string));)*
        };
    }

    macro_rules! headers {
        ($(($NAME:ident, $string:literal)),* $(,)?) => {
            def_header_consts!($(($NAME, $string)),*);
            // Unused except for tests.
            const _ALL: &[&str] = &[$($string),*];
        };
    }

    // Predefined header names, for convenience.
    // Feel free to add to these.
    headers!(
        (ACCEPT_ENCODING, "accept-encoding"),
        (ACCEPT, "accept"),
        (AUTHORIZATION, "authorization"),
        (CONTENT_TYPE, "content-type"),
        (ETAG, "etag"),
        (IF_NONE_MATCH, "if-none-match"),
        (USER_AGENT, "user-agent"),
        // non-standard, but it's convenient to have these.
        (RETRY_AFTER, "retry-after"),
        (X_IF_UNMODIFIED_SINCE, "x-if-unmodified-since"),
        (X_KEY_ID, "x-key-id"),
        (X_LAST_MODIFIED, "x-last-modified"),
        (X_TIMESTAMP, "x-timestamp"),
        (X_WEAVE_NEXT_OFFSET, "x-weave-next-offset"),
        (X_WEAVE_RECORDS, "x-weave-records"),
        (X_WEAVE_TIMESTAMP, "x-weave-timestamp"),
    );

    #[test]
    fn test_predefined() {
        for &name in _ALL {
            assert!(
                HeaderName::new(name).is_ok(),
                "Invalid header name in predefined header constants: {}",
                name
            );
            assert_eq!(
                name.to_ascii_lowercase(),
                name,
                "Non-lowercase name in predefined header constants: {}",
                name
            );
        }
    }

}
