/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::JsonValue;
use rusqlite::types::{FromSql, FromSqlError, ToSql, ToSqlOutput, ValueRef};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Map as JsonMap;
use std::collections::{btree_map, BTreeMap};
use std::iter::FromIterator;
use std::marker::PhantomData;
use std::ops::Index;

use super::Sym;

/// A SymMap is a map with Sym keys.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct SymMap<V>(pub BTreeMap<Sym, V>);

/// A Map where the keys are Syms (interned strings), and values are JsonValue
pub type SymObject = SymMap<JsonValue>;

impl<V> SymMap<V> {
    #[inline]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn contains_key<Q: AsRef<str> + ?Sized>(&self, k: &Q) -> bool {
        self.0.contains_key(k.as_ref())
    }

    pub fn get<Q: AsRef<str> + ?Sized>(&self, k: &Q) -> Option<&V> {
        self.0.get(k.as_ref())
    }

    pub fn get_mut<Q: AsRef<str> + ?Sized>(&mut self, k: &Q) -> Option<&mut V> {
        self.0.get_mut(k.as_ref())
    }

    pub fn remove<Q: AsRef<str> + ?Sized>(&mut self, k: &Q) -> Option<V> {
        self.0.remove(k.as_ref())
    }

    pub fn insert(&mut self, k: impl Into<Sym>, v: V) -> Option<V> {
        self.0.insert(k.into(), v)
    }

    pub fn entry<S>(&mut self, key: impl Into<Sym>) -> btree_map::Entry<'_, Sym, V> {
        self.0.entry(key.into())
    }
}

impl<V> std::ops::Deref for SymMap<V> {
    type Target = BTreeMap<Sym, V>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> std::ops::DerefMut for SymMap<V> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V> Default for SymMap<V> {
    #[inline]
    fn default() -> Self {
        Self(BTreeMap::default())
    }
}

impl<K: Into<Sym>, V, S> From<std::collections::HashMap<K, V, S>> for SymMap<V> {
    fn from(v: std::collections::HashMap<K, V, S>) -> Self {
        Self::from_iter(v)
    }
}

impl<K: Into<Sym>, V> From<BTreeMap<K, V>> for SymMap<V> {
    fn from(v: BTreeMap<K, V>) -> Self {
        Self::from_iter(v)
    }
}

impl<V> IntoIterator for SymMap<V> {
    type Item = (Sym, V);
    type IntoIter = btree_map::IntoIter<Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a SymMap<V> {
    type Item = (&'a Sym, &'a V);
    type IntoIter = btree_map::Iter<'a, Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, V> IntoIterator for &'a mut SymMap<V> {
    type Item = (&'a Sym, &'a mut V);
    type IntoIter = btree_map::IterMut<'a, Sym, V>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl<Q: ?Sized + AsRef<str>, V> Index<&Q> for SymMap<V> {
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

impl<Q: ?Sized + AsRef<str>, V> std::ops::IndexMut<&Q> for SymMap<V> {
    fn index_mut(&mut self, key: &Q) -> &mut V {
        self.get_mut(key).expect("no entry found for key")
    }
}

impl<'a> From<&'a JsonMap<String, JsonValue>> for SymObject {
    fn from(v: &'a JsonMap<String, JsonValue>) -> Self {
        Self(
            v.into_iter()
                .map(|(s, v)| (Sym::new(s), v.clone()))
                .collect(),
        )
    }
}

impl From<SymObject> for JsonValue {
    fn from(o: SymObject) -> Self {
        Self::Object(o.into())
    }
}

impl From<SymObject> for JsonMap<String, JsonValue> {
    fn from(o: SymObject) -> Self {
        o.into_iter().map(|(k, v)| (k.into(), v)).collect()
    }
}

impl std::convert::TryFrom<JsonValue> for SymObject {
    type Error = ();
    fn try_from(o: JsonValue) -> Result<Self, Self::Error> {
        if let JsonValue::Object(o) = o {
            Ok(o.into())
        } else {
            Err(())
        }
    }
}

impl From<JsonMap<String, JsonValue>> for SymObject {
    fn from(o: JsonMap<String, JsonValue>) -> Self {
        o.into_iter().collect()
    }
}

impl<S: Into<Sym>, U> FromIterator<(S, U)> for SymMap<U> {
    fn from_iter<T: IntoIterator<Item = (S, U)>>(iter: T) -> Self {
        SymMap(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl<U> Extend<(Sym, U)> for SymMap<U> {
    fn extend<T: IntoIterator<Item = (Sym, U)>>(&mut self, iter: T) {
        self.0.extend(iter)
    }
}

impl<V: std::fmt::Debug> std::fmt::Debug for SymMap<V> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::fmt::Display for SymObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut writer = crate::util::FormatWriter(f);
        serde_json::to_writer(&mut writer, &self.0).map_err(|_| std::fmt::Error)
    }
}

impl<V: Serialize> Serialize for SymMap<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in self {
            map.serialize_key(k)?;
            map.serialize_value(v)?;
        }
        map.end()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for SymMap<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MapVisitor(PhantomData))
    }
}

impl FromSql for SymObject {
    fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
        if let ValueRef::Text(s) | ValueRef::Blob(s) = value {
            serde_json::from_slice(s).map_err(|err| FromSqlError::Other(Box::new(err)))
        } else {
            Err(FromSqlError::InvalidType)
        }
    }
}

impl ToSql for SymObject {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::from(serde_json::to_string(self).unwrap()))
    }
}

struct MapVisitor<V>(PhantomData<SymMap<V>>);

impl<'de, V: Deserialize<'de>> de::Visitor<'de> for MapVisitor<V> {
    type Value = SymMap<V>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a map")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(SymMap::new())
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut visitor: A) -> Result<Self::Value, A::Error> {
        let mut values = SymMap::new();
        while let Some((key, value)) = visitor.next_entry()? {
            values.0.insert(key, value);
        }
        Ok(values)
    }
}
