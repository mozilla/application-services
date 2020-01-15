/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::borrow::{Borrow, Cow};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::{Index, RangeFull};
use std::sync::Arc;
use string_cache::DefaultAtom;
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
}

impl std::ops::Deref for Sym {
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

impl AsRef<[u8]> for Sym {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}
impl Index<RangeFull> for Sym {
    type Output = str;
    #[inline]
    fn index(&self, _: RangeFull) -> &str {
        self.as_str()
    }
}

// impl<'a> PartialEq<Sym> for &'a Sym {
//     #[inline]
//     fn eq(&self, other: &Sym) -> bool {
//         PartialEq::eq(&self.0, &other.0)
//     }
// }

// impl<'a> PartialEq<&'a Sym> for Sym {
//     #[inline]
//     fn eq(&self, other: &&'a Sym) -> bool {
//         PartialEq::eq(&self.0, &other.0)
//     }
// }

fn as_ref<T: ?Sized + AsRef<str>>(v: &T) -> &str {
    v.as_ref()
    // &v[..]
}

macro_rules! impl_part_eq_cmp {
    (@single $lhs:ty, $rhs:ty, $func:ident) => {
        impl<'a, 'b> PartialEq<$rhs> for $lhs {
            #[inline] fn eq(&self, other: &$rhs) -> bool { PartialEq::eq($func(self), $func(other)) }
        }
        impl<'a, 'b> PartialOrd<$rhs> for $lhs {
            #[inline]
            fn partial_cmp(&self, other: &$rhs) -> Option<Ordering> {
                PartialOrd::partial_cmp($func(self), $func(other))
            }
        }
    };
    ($($rhs:ty),+ $(,)?) => {$(
        impl_part_eq_cmp!{ @single Sym, $rhs, as_ref }
        impl_part_eq_cmp!{ @single $rhs, Sym, as_ref }
        impl_part_eq_cmp!{ @single Sym, &'a $rhs, as_ref }
        impl_part_eq_cmp!{ @single $rhs, &'a Sym, as_ref }
    )+};
}

fn atom(v: &Sym) -> &DefaultAtom {
    &v.0
}

macro_rules! impl_from {
    ($($src:ty),+ $(,)?) => {$(
        impl<'a> From<$src> for Sym {
            fn from(s: $src) -> Self { Self::new(&s[..]) }
        }
    )+};
}

macro_rules! impl_into {
    ($($dst:ty),+ $(,)?) => {$(
        impl From<Sym> for $dst {
            fn from(s: Sym) -> Self { s.as_str().into() }
        }
        impl From<&Sym> for $dst {
            fn from(s: &Sym) -> Self { s.as_str().into() }
        }
    )+};
}

impl_part_eq_cmp!(str, String, Arc<str>);
impl_part_eq_cmp!(@single Sym, &'a Sym, atom);

// impl_ord!(str, &'a str, String);
impl_from!(&'a str, &'a &str, &'a String, &'a &String, Cow<'a, str>);
impl_into!(String, Box<str>, Arc<str>);
// impl_part_eq_cmp!{@single }

impl From<&Sym> for Sym {
    fn from(s: &Sym) -> Self {
        s.clone()
    }
}

impl<'a> From<&'a Sym> for &'a str {
    fn from(s: &'a Sym) -> Self {
        s.as_str()
    }
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

impl serde::Serialize for Sym {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
struct SymVisitor;
impl<'de> serde::de::Visitor<'de> for SymVisitor {
    type Value = Sym;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string")
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Sym::new(v))
    }
}

impl<'de> serde::Deserialize<'de> for Sym {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_str(SymVisitor)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SymMap<V>(pub BTreeMap<Sym, V>);

impl<V> Default for SymMap<V> {
    fn default() -> Self {
        Self(BTreeMap::default())
    }
}

impl<V> std::ops::Deref for SymMap<V> {
    type Target = BTreeMap<Sym, V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> std::ops::DerefMut for SymMap<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V> SymMap<V> {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, k: impl Into<Sym>, v: V) -> Option<V> {
        self.0.insert(k.into(), v)
    }

    pub fn entry<S>(
        &mut self,
        key: impl Into<Sym>,
    ) -> std::collections::btree_map::Entry<'_, Sym, V> {
        self.0.entry(key.into())
    }
}

impl<V> IntoIterator for SymMap<V> {
    type Item = (Sym, V);
    type IntoIter = std::collections::btree_map::IntoIter<Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a SymMap<V> {
    type Item = (&'a Sym, &'a V);
    type IntoIter = std::collections::btree_map::Iter<'a, Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, V> IntoIterator for &'a mut SymMap<V> {
    type Item = (&'a Sym, &'a mut V);
    type IntoIter = std::collections::btree_map::IterMut<'a, Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<Q: ?Sized, V> Index<&Q> for SymMap<V>
where
    Sym: Borrow<Q>,
    Q: Ord,
{
    type Output = V;

    #[inline]
    fn index(&self, key: &Q) -> &V {
        self.0.get(key).expect("no entry found for key")
    }
}
impl<Q: ?Sized, V> std::ops::IndexMut<&Q> for SymMap<V>
where
    Sym: Borrow<Q>,
    Q: Ord,
{
    #[inline]
    fn index_mut(&mut self, key: &Q) -> &mut V {
        self.get_mut(key).expect("no entry found for key")
    }
}

use serde_json::{Map as JsonMap, Value as JsonValue};

impl<'a> From<&'a JsonMap<String, JsonValue>> for SymMap<JsonValue> {
    fn from(v: &'a JsonMap<String, JsonValue>) -> Self {
        Self(
            v.into_iter()
                .map(|(s, v)| (Sym::from(s), v.clone()))
                .collect(),
        )
    }
}

impl<U> std::iter::FromIterator<(Sym, U)> for SymMap<U> {
    fn from_iter<T: IntoIterator<Item = (Sym, U)>>(iter: T) -> Self {
        SymMap(iter.into_iter().collect())
    }
}
impl<V: std::fmt::Debug> std::fmt::Debug for SymMap<V> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl<V: serde::Serialize> serde::Serialize for SymMap<V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in self {
            map.serialize_key(k)?;
            map.serialize_value(v)?;
        }
        map.end()
    }
}

impl<'de, V: serde::Deserialize<'de>> serde::Deserialize<'de> for SymMap<V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
    }
}
struct MapVisitor<V>(std::marker::PhantomData<SymMap<V>>);

impl<'de, V: serde::Deserialize<'de>> serde::de::Visitor<'de> for MapVisitor<V> {
    type Value = SymMap<V>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a map")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(SymMap::new())
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(
        self,
        mut visitor: A,
    ) -> Result<Self::Value, A::Error> {
        let mut values = SymMap::new();
        while let Some((key, value)) = visitor.next_entry()? {
            values.0.insert(key, value);
        }
        Ok(values)
    }
}
