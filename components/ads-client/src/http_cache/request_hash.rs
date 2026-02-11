/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq)]
pub struct RequestHash(String);

impl RequestHash {
    pub fn new(value: &impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        RequestHash(format!("{:x}", hasher.finish()))
    }
}

impl From<&str> for RequestHash {
    fn from(s: &str) -> Self {
        RequestHash(s.to_string())
    }
}

impl From<String> for RequestHash {
    fn from(s: String) -> Self {
        RequestHash(s)
    }
}

impl std::fmt::Display for RequestHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_value_produces_same_hash() {
        let hash1 = RequestHash::new(&("GET", "https://example.com/api"));
        let hash2 = RequestHash::new(&("GET", "https://example.com/api"));
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_values_produce_different_hashes() {
        let hash1 = RequestHash::new(&("GET", "https://example.com/api1"));
        let hash2 = RequestHash::new(&("GET", "https://example.com/api2"));
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_request_hash_from_string() {
        let hash_str = "abc123def456";
        let hash = RequestHash::from(hash_str);
        assert_eq!(hash.to_string(), hash_str);

        let hash_string = String::from("xyz789");
        let hash2 = RequestHash::from(hash_string);
        assert_eq!(hash2.to_string(), "xyz789");
    }
}
