/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::{Borrow, Cow};
use std::ops::{Deref, Index};
use std::sync::Arc;
use string_cache::DefaultAtom;

mod map;
pub use map::{SymMap, SymObject};

/// Sym is a lightweight interned string which can be cloned without sweating
/// too much. It's used for field names, mostly. It's a wrapper around values
/// from servo's string_cache crate, but with slightly more focus on ergonomics
/// and behaving more closely to a string.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Sym(DefaultAtom);

impl Sym {
    #[inline]
    pub fn new(s: &str) -> Self {
        Sym(DefaultAtom::from(s))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &*self.0
    }
    // Clippy doesn't like this, because it means `sym.to_string` won't use
    // `std::fmt::Display`. We know they're the same though, so this just avoids
    // some expensive machinery.
    #[inline]
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        self.as_str().into()
    }
}

impl Deref for Sym {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

// string_cache precomputes the hash code. This is faster, but means it can't
// implement std::borrow::Borrow<str>, which hurts usability with
// HashMap/BTreeMap, in exchange for a faster implementation. I don't really
// care about that case very much here, so manually implment this It's worth
// noting that while PartialEq/PartialOrd/etc have similar optimizations in
// string_cache, they stay compatable with `str`.
#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for Sym {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl Borrow<str> for Sym {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Sym {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
macro_rules! impl_from {
    (@sym_from $($src:ty),+ $(,)?) => {$(
        impl<'a> From<$src> for Sym {
            #[inline]
            fn from(s: $src) -> Self {
                Self::new(&s[..])
            }
        }
    )+};
    (@sym_from_via_atom $($src:ty),+ $(,)?) => {$(
        impl<'a> From<$src> for Sym {
            #[inline]
            fn from(s: $src) -> Self { Self(s.into()) }
        }
    )+};
    (@from_sym $($dst:ty),+ $(,)?) => {$(
        impl From<Sym> for $dst {
            #[inline]
            fn from(s: Sym) -> Self {
                s.as_str().into()
            }
        }
        impl From<&Sym> for $dst {
            #[inline]
            fn from(s: &Sym) -> Self {
                s.as_str().into()
            }
        }
    )+};
}

impl_from!(@sym_from &'a str, &'a &str, &'a String, &'a &String);
impl_from!(@sym_from_via_atom String, Cow<'a, str>);
impl_from!(@from_sym String, Box<str>, Arc<str>, serde_json::Value);

impl From<&Sym> for Sym {
    #[inline]
    fn from(s: &Sym) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a Sym> for &'a str {
    #[inline]
    fn from(s: &'a Sym) -> Self {
        s.as_str()
    }
}

macro_rules! impl_index {
    ($($t:ty),+ $(,)?) => {$(
        impl Index<$t> for Sym {
            type Output = str;
            #[inline]
            fn index(&self, idx: $t) -> &str {
                Index::index(self.as_str(), idx)
            }
        }
    )+};
}

impl_index! {
    std::ops::RangeFull,
    std::ops::Range<usize>,
    std::ops::RangeTo<usize>,
    std::ops::RangeFrom<usize>,
    std::ops::RangeInclusive<usize>,
    std::ops::RangeToInclusive<usize>,
}

// IME the biggest issue with wrapper types is PartialEq not triggering when you
// think it should, so this emits a bunch of variations.
macro_rules! impl_eq {
    ($([$($t:tt)*]),* $(,)?) => { $(impl_eq!{@inner [$($t)*]})* };

    (@inner [$lhs:ty, $rhs:ty; @with_refs]) => {
        impl_eq!(@inner [$lhs, $rhs]);
        impl_eq!(@inner [$lhs, &'a $rhs]);
        impl_eq!(@inner [$rhs, &'a $lhs; @single]);
    };
    (@inner [$lhs:ty, $rhs:ty]) => {
        impl_eq!(@inner [$lhs, $rhs; @single]);
        impl_eq!(@inner [$rhs, $lhs; @single]);
    };
    (@inner [$lhs:ty, $rhs:ty; @single]) => {
        impl_eq!(@emit1 [$lhs, $rhs; @compare (a, b) -> PartialEq::eq(&a[..], &b[..])]);
    };
    (@inner [$lhs:ty, $rhs:ty; @compare ($a:ident, $b:ident) -> $ex:expr]) => {
        impl_eq!(@emit1 [$lhs, $rhs; @compare ($a, $b) -> $ex]);
        impl_eq!(@emit1 [$rhs, $lhs; @compare ($b, $a) -> $ex]);
    };

    (@inner [$lhs:ty, $rhs:ty; @with_refs; @compare ($a:ident, $b:ident) -> $ex:expr]) => {
        impl_eq!(@emit1 [$lhs, $rhs; @compare ($a, $b) -> $ex]);
        impl_eq!(@emit1 [$rhs, $lhs; @compare ($b, $a) -> $ex]);

        impl_eq!(@emit1 [$lhs, &'a $rhs; @compare (a, b) -> <$lhs as PartialEq<$rhs>>::eq(a, *b)]);
        impl_eq!(@emit1 [&'a $rhs, $lhs; @compare (a, b) -> <$rhs as PartialEq<$lhs>>::eq(*a, b)]);
    };
    (@emit1 [$lhs:ty, $rhs:ty; @compare ($a:ident, $b:ident) -> $ex:expr]) => {
        impl<'a> PartialEq<$rhs> for $lhs {
            fn eq(&self, $b: &$rhs) -> bool {
                let $a = self; { $ex }
            }
        }
    };
}

impl_eq! {
    [Sym, str; @with_refs],
    [Sym, String; @with_refs],
    [Sym, Cow<'a, str>],

    [Sym, &'a Sym; @compare (a, b) -> PartialEq::eq(&a.0, &b.0)],

    [Sym, serde_json::Value; @with_refs; @compare (sym, val) -> {
        val.as_str().map_or(false, |s| s == sym.as_str())
    }],

    [SymMap<serde_json::Value>, serde_json::Map<String, serde_json::Value>; @with_refs; @compare (sym_map, json) -> {
        if sym_map.len() != json.len() {
            false
        } else {
            sym_map.iter().all(|(key, value)| json.get(key.as_str()).map_or(false, |v| *value == *v))
        }
    }],
    [SymMap<serde_json::Value>, serde_json::Value; @with_refs; @compare (sym_map, json) -> {
        json.as_object().map_or(false, |o| sym_map == o)
    }],
}

impl std::fmt::Debug for Sym {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for Sym {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_str(), f)
    }
}

impl std::str::FromStr for Sym {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

impl Serialize for Sym {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Sym {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct SymVisitor;

        impl<'de> de::Visitor<'de> for SymVisitor {
            type Value = Sym;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Sym::new(v))
            }
        }

        de.deserialize_str(SymVisitor)
    }
}

impl FromSql for Sym {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_str().map(Sym::new)
    }
}

impl ToSql for Sym {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        self.as_str().to_sql()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    // ensures Deref isn't invoked.
    fn check_eq<A, B>(a: &A, b: &B)
    where
        A: PartialEq<B> + std::fmt::Debug + ?Sized,
        B: std::fmt::Debug + PartialEq<A> + ?Sized,
    {
        // intentionally not assert_eq.
        assert!(a == b, "{:?} != {:?}", a, b);
        assert!(b == a, "{:?} != {:?}", b, a);
    }

    #[test]
    fn test_sym_eq() {
        let test: Sym = Sym::new("test");
        let a = &"test".to_string();
        check_eq(&test, &json!("test"));
        check_eq(&test, a);
        check_eq(&test, &a);
        check_eq(&test, "test");
        check_eq(&test, &"test");
        check_eq(&test, &Cow::Borrowed("test"));
        let test2 = Sym::new("test");
        check_eq(&test, &&test2);
    }

    #[test]
    #[allow(clippy::useless_format)]
    fn test_sym_misc() {
        assert_eq!(
            format!("{}", Sym::new("foo")),
            format!("{}", String::from("foo"))
        );
        assert_eq!(
            format!("{:?}", Sym::new("foo")),
            format!("{:?}", String::from("foo"))
        );
        assert_eq!("foo".parse::<Sym>().unwrap(), "foo");
    }

    #[test]
    fn test_sym_serde() {
        let test: Sym = serde_json::from_value(json!("test")).unwrap();
        assert_eq!(test, "test");
        let testv: Vec<Sym> = serde_json::from_value(json!(["test", "one", "two"])).unwrap();
        assert_eq!(testv, ["test", "one", "two"]);
        let val = serde_json::to_value(Sym::new("test")).unwrap();
        assert_eq!(val, serde_json::Value::String("test".into()));
    }

    #[test]
    fn test_hash_borrow() {
        // if we derive Hash, this fails.
        let v: std::collections::HashSet<Sym> = ["foo", "bar", "1", "2", "3"]
            .iter()
            .map(Sym::from)
            .collect();
        assert!(v.contains("foo"));
        assert!(v.contains("bar"));
        assert!(v.contains("1"));
        assert!(v.contains("2"));
        assert!(v.contains("3"));
    }

    #[test]
    fn test_index() {
        let x = Sym::from("abcde");
        assert_eq!(&x[..], "abcde");
        assert_eq!(&x[1..], "bcde");
        assert_eq!(&x[..4], "abcd");
        assert_eq!(&x[..=3], "abcd");
        assert_eq!(&x[1..4], "bcd");
        assert_eq!(&x[1..=3], "bcd");
    }

    #[test]
    fn test_into() {
        fn check_sym_from(v: impl Into<Sym>) {
            let x: Sym = v.into();
            assert_eq!("abc", x);
        }
        check_sym_from("abc".to_string());
        check_sym_from(&"abc".to_string());
        check_sym_from(&"abc");
        check_sym_from("abc");
        check_sym_from(Sym::new("abc"));
        check_sym_from(&Sym::new("abc"));
        check_sym_from(Cow::Borrowed("abc"));
        check_sym_from(Cow::Owned("abc".into()));
        check_sym_from(Cow::Borrowed("abc"));

        fn check_from_sym<T>(v: Sym)
        where
            T: From<Sym> + AsRef<str>,
        {
            assert_eq!("abc", T::from(v).as_ref());
        }
        fn check_from_sym_ref<'a, T>(v: &'a Sym)
        where
            T: From<&'a Sym> + AsRef<str>,
        {
            assert_eq!("abc", T::from(v).as_ref());
        }

        let abc = Sym::new("abc");

        check_from_sym_ref::<String>(&abc);
        check_from_sym_ref::<Box<str>>(&abc);
        check_from_sym_ref::<Arc<str>>(&abc);

        check_from_sym_ref::<&str>(&abc);
        check_from_sym_ref::<Sym>(&abc);

        check_from_sym::<String>(abc.clone());
        check_from_sym::<Box<str>>(abc.clone());
        check_from_sym::<Arc<str>>(abc.clone());

        assert_eq!(serde_json::Value::from(abc.clone()).as_str(), Some("abc"));
        assert_eq!(serde_json::Value::from(&abc).as_str(), Some("abc"));
    }
}
