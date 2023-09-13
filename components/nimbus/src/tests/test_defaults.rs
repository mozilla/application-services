/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// cargo test --package nimbus-sdk --lib --all-features -- tests::test_defaults --nocapture

use crate::{defaults::Defaults, error::Result, NimbusError::InternalError};
use serde_json::{json, Value};
use std::collections::HashMap;

#[cfg(test)]
impl Defaults for &str {
    fn defaults(&self, fb: &Self) -> Result<Self> {
        if self.starts_with("err") || fb.starts_with("err") {
            Err(InternalError("OMG Error"))
        } else {
            Ok(self)
        }
    }
}

#[test]
fn test_defaults_test_impl() -> Result<()> {
    // This implementation for &str doesn't exist in non-test code.
    // We have it, and test it here in order to make testing of error
    // recovery in real implementations.
    let (a, b) = ("ok", "yes");
    assert_eq!(a.defaults(&b)?, a);

    let (a, b) = ("err", "yes");
    assert!(a.defaults(&b).is_err());

    let (a, b) = ("yes", "err");
    assert!(a.defaults(&b).is_err());

    Ok(())
}

#[test]
fn test_defaults_optional() -> Result<()> {
    let (a, b) = (Some("a"), Some("b"));
    assert_eq!(a.defaults(&b)?, a);

    let (a, b) = (None, Some("b"));
    assert_eq!(a.defaults(&b)?, b);

    let (a, b) = (Some("a"), None);
    assert_eq!(a.defaults(&b)?, a);

    let (a, b) = (None as Option<&str>, None);
    assert_eq!(a.defaults(&b)?, a);

    Ok(())
}

#[test]
fn test_defaults_hashmap() -> Result<()> {
    let a = HashMap::<String, &str>::from_iter([
        ("a".to_string(), "A from a"),
        ("b".to_string(), "B from a"),
    ]);

    let b = HashMap::<String, &str>::from_iter([
        ("a".to_string(), "AA not replaced"),
        ("b".to_string(), "errBB merge failed, so omitting"),
        ("c".to_string(), "CC added"),
        ("d".to_string(), "errDD not merged, but added"),
    ]);

    let exp = HashMap::<String, &str>::from_iter([
        ("a".to_string(), "A from a"),
        // we tried to merge the defaults, but it failed, so we
        // we keep the original (i.e. the experiment rather than the rollout)
        ("b".to_string(), "B from a"),
        ("c".to_string(), "CC added"),
        ("d".to_string(), "errDD not merged, but added"),
    ]);

    assert_eq!(a.defaults(&b)?, exp);

    Ok(())
}

#[test]
fn test_defaults_json() -> Result<()> {
    // missing keys are added from the defaults.
    let (a, b) = (json!({}), json!({"a": 1}));
    assert_eq!(a.defaults(&b)?, json!({ "a": 1 }));

    // new nulls remove the defaults.
    let (a, b) = (json!({ "a": null }), json!({ "a": 1 }));
    assert_eq!(a.defaults(&b)?, json!({}));

    // default nulls are overridden.
    let (a, b) = (json!({ "a": 1 }), json!({ "a": null }));
    assert_eq!(a.defaults(&b)?, json!({ "a": 1 }));

    // non-object values are not overridden.
    let (a, b) = (json!({ "a": 1 }), json!({ "a": 2 }));
    assert_eq!(a.defaults(&b)?, json!({ "a": 1 }));

    let (a, b) = (json!({ "a": true }), json!({ "a": false }));
    assert_eq!(a.defaults(&b)?, json!({ "a": true }));

    let (a, b) = (json!({ "a": "foo" }), json!({ "a": "bar" }));
    assert_eq!(a.defaults(&b)?, json!({ "a": "foo" }));

    let (a, b) = (json!({ "a": [] }), json!({ "a": [1] }));
    assert_eq!(a.defaults(&b)?, json!({ "a": [] }));

    // types do not have to match; we only want to pass through.
    let (a, b) = (json!({ "a": [] }), json!({ "a": 1 }));
    assert_eq!(a.defaults(&b)?, json!({ "a": [] }));

    let (a, b) = (json!({ "a": 1 }), json!({ "a": [] }));
    assert_eq!(a.defaults(&b)?, json!({ "a": 1 }));

    // Values which are objects are recursively merged.
    let (a, b) = (
        json!({ "a": { "a": 1 } }),
        json!({ "a": { "a": 2, "b": 2 } }),
    );
    assert_eq!(a.defaults(&b)?, json!({ "a": { "a": 1, "b": 2 } }));

    Ok(())
}

#[test]
fn test_defaults_maps_of_json() -> Result<()> {
    let exp_bob = json!({
        "specified": "Experiment in part".to_string(),
    });
    let mut exp_map: HashMap<String, Value> = Default::default();
    exp_map.insert("bob".to_string(), exp_bob.clone());

    let ro_bob = json!({
        "name": "Bob".to_string(),
        "specified": "Rollout".to_string(),
    });
    let mut ro_map: HashMap<String, Value> = Default::default();
    ro_map.insert("bob".to_string(), ro_bob.clone());

    let map = exp_map.defaults(&ro_map)?;

    assert_eq!(
        map["bob"],
        json!({
            "name": "Bob".to_string(),
            "specified": "Experiment in part".to_string(),
        })
    );

    // optional JSON
    let exp_bob = Some(exp_bob);
    let ro_bob = Some(ro_bob);

    let bob = exp_bob.defaults(&ro_bob)?;
    assert_eq!(
        bob,
        Some(json!({
            "name": "Bob".to_string(),
            "specified": "Experiment in part".to_string(),
        }))
    );

    Ok(())
}

#[test]
fn test_defaults_realistic_json() -> Result<()> {
    let a = json!({
        "items-enabled": {
            "b": false,
            "c": false,
        },
        "items": {
            "a": {
                "title": "Capital A",
            },
        }
    });

    let b = json!({
        "items-enabled": {
            "a": true,
            "b": true,
            "c": true,
            "d": true,
        },
        "ordering": ["a", "b", "c", "d"],
        "items": {
            "a": {
                "title": "A",
                "link": "a",
            },
            "b": {
                "title": "B",
                "link": "b",
            },
            "c": {
                "title": "C",
                "link": "c",
            },
            "d": {
                "title": "D",
                "link": "d",
            },
        }
    });

    let exp = json!({
        "items-enabled": {
            "a": true,
            "b": false,
            "c": false,
            "d": true,
        },
        "ordering": ["a", "b", "c", "d"],
        "items": {
            "a": {
                "title": "Capital A",
                "link": "a",
            },
            "b": {
                "title": "B",
                "link": "b",
            },
            "c": {
                "title": "C",
                "link": "c",
            },
            "d": {
                "title": "D",
                "link": "d",
            },
        }
    });

    assert_eq!(a.defaults(&b)?, exp);

    Ok(())
}
