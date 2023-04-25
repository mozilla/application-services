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

pub(crate) fn try_find_experiment(value: &Value, slug: &str) -> Result<Value> {
    let array = try_extract_data_list(value)?;
    let exp = array
        .iter()
        .find(|exp| {
            if let Some(Value::String(s)) = exp.get("slug") {
                slug == s
            } else {
                false
            }
        })
        .ok_or_else(|| anyhow::Error::msg(format!("No experiment with slug {}", slug)))?;

    Ok(exp.clone())
}

pub(crate) fn try_extract_data_list(value: &Value) -> Result<Vec<Value>> {
    assert!(value.is_object());
    Ok(value.get_array("data")?.to_vec())
}

pub(crate) fn prepare_experiment(
    experiment: &Value,
    slug: &str,
    channel: &str,
    branch: &str,
    preserve_targeting: bool,
    preserve_bucketing: bool,
) -> Result<Value> {
    let mut experiment = experiment.clone();
    experiment.set("channel", channel)?;
    experiment.set("isEnrollmentPaused", false)?;
    if !preserve_targeting {
        experiment.set("targeting", "true")?;
    }

    if !preserve_bucketing {
        let bucketing = experiment.get_mut_object("bucketConfig")?;
        bucketing.set("start", 0)?;
        bucketing.set("count", 10_000)?;

        let branches = experiment.get_mut_array("branches")?;
        let mut found = false;
        for b in branches {
            let slug = b.get_str("slug")?;
            let ratio = if slug == branch {
                found = true;
                100
            } else {
                0
            };
            b.set("ratio", ratio)?;
        }
        if !found {
            anyhow::bail!(format!(
                "No branch called '{}' was found in '{}'",
                branch, slug
            ));
        }
    }
    Ok(experiment)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_find_experiment() -> Result<()> {
        let exp = json!({
            "slug": "a-name",
        });
        let source = json!({ "data": [exp] });

        assert_eq!(try_find_experiment(&source, "a-name")?, exp);

        let source = json!({
            "data": {},
        });
        assert!(try_find_experiment(&source, "a-name").is_err());

        let source = json!({
            "data": [],
        });
        assert!(try_find_experiment(&source, "a-name").is_err());

        Ok(())
    }

    #[test]
    fn test_prepare_experiment() -> Result<()> {
        let src = json!({
            "slug": "a-name",
            "branches": [
                {
                    "slug": "another-branch",
                },
                {
                    "slug": "a-branch",
                }
            ],
            "bucketConfig": {
            }
        });

        assert_eq!(
            json!({
                "channel": "developer",
                "slug": "a-name",
                "branches": [
                    {
                        "slug": "another-branch",
                        "ratio": 0,
                    },
                    {
                        "slug": "a-branch",
                        "ratio": 100,
                    }
                ],
                "bucketConfig": {
                    "start": 0,
                    "count": 10_000,
                },
                "isEnrollmentPaused": false,
                "targeting": "true"
            }),
            prepare_experiment(&src, "a-name", "developer", "a-branch", false, false)?
        );

        assert_eq!(
            json!({
                "channel": "developer",
                "slug": "a-name",
                "branches": [
                    {
                        "slug": "another-branch",
                    },
                    {
                        "slug": "a-branch",
                    }
                ],
                "bucketConfig": {
                },
                "isEnrollmentPaused": false,
                "targeting": "true"
            }),
            prepare_experiment(&src, "a-name", "developer", "a-branch", false, true)?
        );

        assert_eq!(
            json!({
                "channel": "developer",
                "slug": "a-name",
                "branches": [
                    {
                        "slug": "another-branch",
                        "ratio": 0,
                    },
                    {
                        "slug": "a-branch",
                        "ratio": 100,
                    }
                ],
                "bucketConfig": {
                    "start": 0,
                    "count": 10_000,
                },
                "isEnrollmentPaused": false,
            }),
            prepare_experiment(&src, "a-name", "developer", "a-branch", true, false)?
        );

        assert_eq!(
            json!({
                "slug": "a-name",
                "channel": "developer",
                "branches": [
                    {
                        "slug": "another-branch",
                    },
                    {
                        "slug": "a-branch",
                    }
                ],
                "bucketConfig": {
                },
                "isEnrollmentPaused": false,
            }),
            prepare_experiment(&src, "a-name", "developer", "a-branch", true, true)?
        );
        Ok(())
    }
}
