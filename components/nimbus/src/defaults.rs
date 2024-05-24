/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::error::Result;

/// Simple trait to allow merging of similar objects.
///
/// Different names might be more applicable: merging, defaulting, patching.
///
/// In all cases: the `defaults` method takes a reference to a `self` and
/// a `fallback`. The `self` acts as a patch over the `fallback`, and a new
/// version is the result.
///
/// Implementations of the trait can error. In the case of recursive implementations,
/// other implementations may catch and recover from the error, or propagate it.
///
/// Context: Feature JSON is used to configure a application feature.
/// If a value is needed, the application provides a default.
/// A rollout changes this default.
pub trait Defaults {
    fn defaults(&self, fallback: &Self) -> Result<Self>
    where
        Self: Sized;
}

impl<T: Defaults + Clone> Defaults for Option<T> {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        Ok(match (self, fallback) {
            (Some(a), Some(b)) => Some(a.defaults(b)?),
            (Some(_), None) => self.clone(),
            _ => fallback.clone(),
        })
    }
}

use serde_json::{Map, Value};
/// We implement https://datatracker.ietf.org/doc/html/rfc7396
/// such that self is patching the fallback.
/// The result is the patched object.
///
/// * If a self value is null, we take that to equivalent to a delete.
/// * If both self and fallback are objects, we recursively patch.
/// * If it exists in either in self or clone, then it is included
/// * in the result.
/// * If it exists in both, then we take the self version.
impl Defaults for Value {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        Ok(match (self, fallback) {
            (Value::Object(a), Value::Object(b)) => Value::Object(a.defaults(b)?),
            (Value::Null, _) => fallback.to_owned(),
            _ => self.to_owned(),
        })
    }
}

impl Defaults for Map<String, Value> {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        let mut map = self.clone();
        for (k, fb) in fallback {
            match map.get(k) {
                Some(existing) if existing.is_null() => {
                    map.remove(k);
                }
                Some(existing) => {
                    // JSON merging should't error, so there'll be
                    // nothing to propagate.
                    map[k] = existing.defaults(fb)?;
                }
                _ => {
                    map.insert(k.clone(), fb.clone());
                }
            };
        }
        Ok(map)
    }
}

use std::collections::HashMap;
/// Merge the two `HashMap`s, with self acting as the dominant
/// of the two.
///
/// Nimbus' use case is to be merging data coming from the outside,
/// we should not allow a bad merge to bring the whole system down.
///
/// Where values merging fails, we go with the newest version.
impl<T: Defaults + Clone> Defaults for HashMap<String, T> {
    fn defaults(&self, fallback: &Self) -> Result<Self> {
        let mut map = self.clone();
        for (k, fb) in fallback {
            match map.get(k) {
                Some(existing) => {
                    // if we merged with fb without errors,
                    if let Ok(v) = existing.defaults(fb) {
                        map.insert(k.clone(), v);
                    } // otherwise use the self value, without merging.
                }
                _ => {
                    map.insert(k.clone(), fb.clone());
                }
            }
        }
        Ok(map)
    }
}
