// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

pub(crate) trait CliUtils {
    fn get_str<'a>(&'a self, key: &str) -> Result<&'a str>;
    fn get_array<'a>(&'a self, key: &str) -> Result<&'a Vec<Value>>;
    fn get_mut_array<'a>(&'a mut self, key: &str) -> Result<&'a mut Vec<Value>>;
    fn get_mut_object<'a>(&'a mut self, key: &str) -> Result<&'a mut Value>;

    fn set<V>(&mut self, key: &str, value: V) -> Result<()>
    where
        V: Serialize;
}

impl CliUtils for Value {
    fn get_str<'a>(&'a self, key: &str) -> Result<&'a str> {
        let v = self
            .get(key)
            .ok_or_else(|| anyhow::Error::msg("Expected a string in the JSONObject"))?
            .as_str()
            .ok_or_else(|| anyhow::Error::msg("value is not a string"))?;

        Ok(v)
    }

    fn get_array<'a>(&'a self, key: &str) -> Result<&'a Vec<Value>> {
        let v = self
            .get(key)
            .ok_or_else(|| anyhow::Error::msg("Expected an array in the JSONObject"))?
            .as_array()
            .ok_or_else(|| anyhow::Error::msg("value is not a array"))?;
        Ok(v)
    }

    fn get_mut_array<'a>(&'a mut self, key: &str) -> Result<&'a mut Vec<Value>> {
        let v = self
            .get_mut(key)
            .ok_or_else(|| anyhow::Error::msg("Expected an array in the JSONObject"))?
            .as_array_mut()
            .ok_or_else(|| anyhow::Error::msg("value is not a array"))?;
        Ok(v)
    }

    fn get_mut_object<'a>(&'a mut self, key: &str) -> Result<&'a mut Value> {
        let v = self
            .get_mut(key)
            .ok_or_else(|| anyhow::Error::msg("Expected an array in the JSONObject"))?;
        Ok(v)
    }

    fn set<V>(&mut self, key: &str, value: V) -> Result<()>
    where
        V: Serialize,
    {
        let value = serde_json::to_value(value)?;
        match self.as_object_mut() {
            Some(m) => m.insert(key.to_string(), value),
            _ => anyhow::bail!("Can only insert into JSONObjects"),
        };
        Ok(())
    }
}
