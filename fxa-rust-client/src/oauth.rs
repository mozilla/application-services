/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use errors::*;
use regex::Regex;
use url::Url;

lazy_static! {
    // These are additional restrictions that FxA imposes.
    // These character ranges are from the OAuth RFC,
    // https://tools.ietf.org/html/rfc6749#section-3.3
    static ref VALID_SCOPE_VALUE: Regex = Regex::new(r"^[\x21\x23-\x5B\x5D-\x7E]+$").unwrap();
    static ref VALID_SHORT_NAME_VALUE: Regex = Regex::new(r"^[a-zA-Z0-9_]+$").unwrap();
    static ref VALID_FRAGMENT_VALUE: Regex = Regex::new(r"^[a-zA-Z0-9_]+$").unwrap();
    static ref SCOPE_STRING_SEPARATOR: Regex = Regex::new(r"[ ]+").unwrap();
}

enum ScopeValue {
    ShortScope(ShortScopeValue),
    URLScope(URLScopeValue),
}

struct ShortScopeValue {
    has_write: bool,
    names: Vec<String>,
}

impl ShortScopeValue {
    pub fn new(value: &str) -> Result<ShortScopeValue> {
        if !VALID_SCOPE_VALUE.is_match(value) {
            return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
        }
        let mut names = value
            .split(":")
            .map(|value| value.to_string())
            .collect::<Vec<String>>();
        for name in &names {
            if !VALID_SHORT_NAME_VALUE.is_match(name) {
                return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
            }
        }
        let has_write = match names.last() {
            Some(last_item) => match last_item.as_ref() {
                "write" => {
                    if names.len() == 1 {
                        // write was the last and only item.
                        return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
                    }
                    true
                }
                _ => false,
            },
            None => return Err(ErrorKind::EmptyOAuthScopeNames.into()),
        };
        if has_write {
            names.pop();
        }
        Ok(ShortScopeValue { has_write, names })
    }

    fn implies(&self, value: &ShortScopeValue) -> bool {
        if value.has_write && !self.has_write {
            return false;
        }
        if value.names.len() < self.names.len() {
            return false;
        }
        self.names
            .iter()
            .zip(value.names.iter())
            .all(|(a, b)| a == b)
    }

    fn to_string(&self) -> String {
        let mut str = self.names.join(":");
        if self.has_write {
            str.push_str(":write");
        }
        str
    }
}

struct URLScopeValue {
    url: Url,
}

impl URLScopeValue {
    pub fn new(value: &str) -> Result<URLScopeValue> {
        if !VALID_SCOPE_VALUE.is_match(value) {
            return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
        }
        let url = Url::parse(value)?;
        if url.scheme() != "https" {
            return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
        }
        if url.username().len() > 0 || url.password().is_some() || url.query().is_some() {
            return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
        }
        if let Some(fragment) = url.fragment() {
            if !VALID_FRAGMENT_VALUE.is_match(fragment) {
                return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
            }
        }
        if url.to_string() != value {
            return Err(ErrorKind::InvalidOAuthScopeValue(value.to_string()).into());
        }
        Ok(URLScopeValue { url })
    }

    fn implies(&self, value: &URLScopeValue) -> bool {
        if value.url.origin() != self.url.origin() {
            return false;
        }
        if value.url.path() != self.url.path() {
            let own_path = format!("{}/", self.url.path());
            if !value.url.path().starts_with(&own_path) {
                return false;
            }
        }
        if let Some(own_fragment) = self.url.fragment() {
            match value.url.fragment() {
                Some(fragment) => {
                    if fragment != own_fragment {
                        return false;
                    }
                }
                None => {
                    return false;
                }
            }
        }
        return true;
    }

    fn to_string(&self) -> String {
        self.url.to_string()
    }
}

impl ScopeValue {
    fn implies(&self, scope_value: &ScopeValue) -> bool {
        match self {
            ScopeValue::ShortScope(ref own) => match scope_value {
                ScopeValue::ShortScope(ref other) => own.implies(other),
                _ => false,
            },
            ScopeValue::URLScope(ref own) => match scope_value {
                ScopeValue::URLScope(ref other) => own.implies(other),
                _ => false,
            },
        }
    }

    fn to_string(&self) -> String {
        match self {
            ScopeValue::ShortScope(value) => value.to_string(),
            ScopeValue::URLScope(value) => value.to_string(),
        }
    }
}

pub struct Scope {
    values: Vec<Box<ScopeValue>>,
}

impl Scope {
    /// Parse a space-delimited string into a Scope object.
    ///
    /// This function implements the semantics defined in RFC6749,
    /// where the "scope" input string represents a space-delimited
    /// list of case-sensitive strings identifying individual scopes.
    pub fn from_string(scope_str: &str) -> Result<Scope> {
        let values_str: Vec<&str> = SCOPE_STRING_SEPARATOR
            .split(scope_str)
            .filter(|s| s.len() > 0)
            .collect();
        let mut values: Vec<Box<ScopeValue>> = Vec::with_capacity(values_str.len());
        for value in values_str {
            if value.starts_with("https:") {
                values.push(Box::new(ScopeValue::URLScope(URLScopeValue::new(value)?)));
            } else {
                values.push(Box::new(ScopeValue::ShortScope(ShortScopeValue::new(
                    value,
                )?)));
            }
        }
        Ok(Scope { values })
    }

    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        self.values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<String>>()
            .join(" ")
    }

    /// Check whether one scope implies another.
    ///
    /// A scope A implies another scope B if every scope value
    /// in B is implied by some scope value in A.  In other words,
    /// A represents a strictly more general set of capabilities
    /// than B.
    pub fn implies(&self, scope: &Scope) -> bool {
        for target in &scope.values {
            let mut is_implied = false;
            for value in &self.values {
                if value.implies(target.as_ref()) {
                    is_implied = true;
                    break;
                }
            }
            if !is_implied {
                return false;
            }
        }
        return true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_implications() {
        let valid_implications = [
            // [A, B] => "A implies B"
            ("profile:write", "profile"),
            ("profile", "profile:email"),
            ("profile:write", "profile:email"),
            ("profile:write", "profile:email:write"),
            ("profile:email:write", "profile:email"),
            ("profile profile:email:write", "profile:email"),
            ("profile profile:email:write", "profile:display_name"),
            (
                "profile https://identity.mozilla.com/apps/oldsync",
                "profile",
            ),
            ("foo bar:baz", "foo:dee"),
            ("foo bar:baz", "bar:baz"),
            ("foo bar:baz", "foo:mah:pa bar:baz:quux"),
            (
                "profile https://identity.mozilla.com/apps/oldsync",
                "https://identity.mozilla.com/apps/oldsync",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync",
                "https://identity.mozilla.com/apps/oldsync#read",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync",
                "https://identity.mozilla.com/apps/oldsync/bookmarks",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync",
                "https://identity.mozilla.com/apps/oldsync/bookmarks#read",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync#read",
                "https://identity.mozilla.com/apps/oldsync/bookmarks#read",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync#read profile",
                "https://identity.mozilla.com/apps/oldsync/bookmarks#read",
            ),
        ];
        for (source, target) in &valid_implications {
            let source = Scope::from_string(source).unwrap();
            let target = Scope::from_string(target).unwrap();
            assert!(source.implies(&target));
        }
    }

    #[test]
    fn test_invalid_implications() {
        let invalid_implications = [
            // [A, B] => "A does not imply B"
            ("profile:email:write", "profile"),
            ("profile:email:write", "profile:write"),
            ("profile:email", "profile:display_name"),
            ("profilebogey", "profile"),
            ("foo bar:baz", "bar"),
            ("profile:write", "https://identity.mozilla.com/apps/oldsync"),
            ("profile profile:email:write", "profile:write"),
            ("https", "https://identity.mozilla.com/apps/oldsync"),
            ("https://identity.mozilla.com/apps/oldsync", "profile"),
            (
                "https://identity.mozilla.com/apps/oldsync#read",
                "https://identity.mozila.com/apps/oldsync/bookmarks",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync#write",
                "https://identity.mozila.com/apps/oldsync/bookmarks#read",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync/bookmarks",
                "https://identity.mozila.com/apps/oldsync",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync/bookmarks",
                "https://identity.mozila.com/apps/oldsync/passwords",
            ),
            (
                "https://identity.mozilla.com/apps/oldsyncer",
                "https://identity.mozila.com/apps/oldsync",
            ),
            (
                "https://identity.mozilla.com/apps/oldsync",
                "https://identity.mozila.com/apps/oldsyncer",
            ),
            (
                "https://identity.mozilla.org/apps/oldsync",
                "https://identity.mozila.com/apps/oldsync",
            ),
        ];
        for (source, target) in &invalid_implications {
            let source = Scope::from_string(source).unwrap();
            let target = Scope::from_string(target).unwrap();
            assert!(!source.implies(&target));
        }
    }

    #[test]
    fn test_scope_values() {
        let invalid_scope_values = [
            "profile:email!:write",
            ":",
            "::",
            "write",
            ":profile",
            "profile::email",
            "profile profile\0:email",
            "https://foo@identity.mozilla.com/apps/oldsync",
            "https://foo:bar@identity.mozilla.com/apps/oldsync",
            "https://identity.mozilla.com/apps/oldsync?foo=bar",
            "http://identity.mozilla.com/apps/oldsync",
            "file:///etc/passwd",
            "https://identity.mozilla.com/apps/oldsync/../notes",
            "http://identity.mozilla.com/apps/oldsync#read!",
            "http://identity.mozilla.com/apps/oldsync#read+write",
        ];
        for source in &invalid_scope_values {
            assert!(Scope::from_string(source).is_err());
        }
    }
}
