/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::borrow::Borrow;
use std::sync::Arc;
use std::{fmt, ops};
use serde::{ser, de};

// We use Arc and not Rc because error_chain requires Send. This is annoying
// but unlikely to matter.

/// Represents a record identifier. This is immutable, provides additional type safety
/// over strings, and also should have better memory usage and be cheaper to clone.
#[derive(Clone, Default, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Id(pub Arc<String>);

impl Id {
    #[inline]
    pub fn new(s: String) -> Id {
        Id(Arc::new(s))
    }

    #[inline]
    pub fn from_str(s: &str) -> Id {
        Id::new(s.into())
    }

    #[inline]
    pub fn to_string(&self) -> String {
        (*self.0).clone()
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Transparently serialize to a string when used with serde.
impl ser::Serialize for Id {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_ref())
    }
}

/// Transparently deserialize from a string when used with serde.
impl<'de> de::Deserialize<'de> for Id {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Id, D::Error> {
        String::deserialize(deserializer).map(Id::new)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Id({:?})", self.as_str())
    }
}

impl ops::Deref for Id {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        &self.0[..]
    }
}

// Implement == and != operators.

macro_rules! impl_eq {
    ($(($a:ty, $b:ty)),+ $(,)*) => {$(
        impl<'a> PartialEq<$b> for $a {
            #[inline]
            fn eq(&self, other: &$b) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
        }

        impl<'a> PartialEq<$a> for $b {
            #[inline]
            fn eq(&self, other: &$a) -> bool {
                PartialEq::eq(&self[..], &other[..])
            }
        }
    )+};
}

impl_eq! { (Id, str), (Id, &'a str), (Id, String), (Id, &'a String) }

// Implement a bunch of conversions to/from str and String.

impl From<String> for Id {
    #[inline]
    fn from(s: String) -> Id {
        Id(Arc::new(s))
    }
}

impl From<Id> for String {
    #[inline]
    fn from(id: Id) -> String {
        (*id.0).clone()
    }
}

impl<'a> From<&'a str> for Id {
    #[inline]
    fn from(s: &'a str) -> Id {
        Id(Arc::new(String::from(s)))
    }
}

impl<'a> From<&'a String> for Id {
    #[inline]
    fn from(s: &'a String) -> Id {
        Id(Arc::new(s.clone()))
    }
}

impl AsRef<str> for Id {
    #[inline] fn as_ref(&self) -> &str { self.0.as_ref() }
}

impl AsRef<String> for Id {
    #[inline] fn as_ref(&self) -> &String { &self.0 }
}

impl Borrow<str> for Id {
    #[inline] fn borrow(&self) -> &str { &*self }
}

impl Borrow<String> for Id {
    #[inline] fn borrow(&self) -> &String { &*self.0 }
}

/// Allow &id[..] to work for consistency with other string-likes. There's probably not
/// a strong reason to implement other range operator overloads though.
impl ops::Index<ops::RangeFull> for Id {
    type Output = str;
    #[inline]
    fn index(&self, _: ops::RangeFull) -> &str {
        self.0.as_str()
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use serde_json;
    #[test]
    fn test_eq() {
        assert_eq!(Id::from_str("abcd"), "abcd");
        assert_eq!(Id::from_str("abcd"), "abcd".to_string());
        assert_eq!(Id::from_str("abcd"), &"abcd".to_string());

        assert_ne!(Id::from_str("ab"), "abcd");
        assert_ne!(Id::from_str("ab"), "abcd".to_string());
        assert_ne!(Id::from_str("ab"), &"abcd".to_string());
    }

    #[test]
    fn test_serde() {
        #[derive(Serialize, Deserialize)]
        struct HasId {
            id: Id,
        }

        let json = r#"{"id":"foo"}"#;
        let res: HasId = serde_json::from_str(json).unwrap();
        assert_eq!(res.id, "foo");
        let s = serde_json::to_string(&res).unwrap();
        assert_eq!(s, json);
    }
}
