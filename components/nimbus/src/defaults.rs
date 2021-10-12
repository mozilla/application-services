/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;
pub trait Defaults {
    fn defaults(&self, defaults: &Self) -> Result<Self>
    where
        Self: Sized;
}

impl<T: Defaults + Clone> Defaults for Option<T> {
    fn defaults(&self, defaults: &Self) -> Result<Self> {
        Ok(match (self, defaults) {
            (Some(a), Some(b)) => Some(a.defaults(b)?),
            (Some(_), None) => self.clone(),
            _ => defaults.clone(),
        })
    }
}

use serde_json::{Map, Value};
impl Defaults for Value {
    fn defaults(&self, defaults: &Self) -> Result<Self> {
        Ok(match (self, defaults) {
            (Value::Object(a), Value::Object(b)) => Value::Object(a.defaults(b)?),
            (Value::Null, _) => defaults.to_owned(),
            _ => self.to_owned(),
        })
    }
}

impl Defaults for Map<String, Value> {
    fn defaults(&self, defaults: &Self) -> Result<Self> {
        let mut map = self.clone();
        for (k, v) in defaults {
            map[k] = map[k].defaults(v)?;
        }
        Ok(map)
    }
}

use std::collections::HashMap;
impl<T: Defaults + Clone> Defaults for HashMap<String, T> {
    fn defaults(&self, defaults: &Self) -> Result<Self> {
        let mut map = self.clone();
        for k in defaults.keys() {
            let v = map[k].defaults(&defaults[k])?;
            map.insert(k.to_string(), v);
        }
        Ok(map)
    }
}
