/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{error::Result, FeatureConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfigProposed {
    pub enabled: bool,
    pub feature_id: String,
    #[serde(default)]
    pub value: Map<String, Value>,
}

#[test]
fn test_deserialize_untyped_json() -> Result<()> {
    let without_value = serde_json::from_value::<FeatureConfig>(json!(
        {
            "featureId": "some_control",
            "enabled": true,
        }
    ))?;

    let with_object_value = serde_json::from_value::<FeatureConfig>(json!(
        {
            "featureId": "some_control",
            "enabled": true,
            "value": {
                "color": "blue",
            },
        }
    ))?;

    assert_eq!(
        serde_json::to_string(&without_value.value)?,
        "{}".to_string()
    );
    assert_eq!(
        serde_json::to_string(&with_object_value.value)?,
        "{\"color\":\"blue\"}"
    );
    assert_eq!(with_object_value.value.get("color").unwrap(), "blue");

    let rejects_scalar_value = serde_json::from_value::<FeatureConfig>(json!(
        {
            "featureId": "some_control",
            "enabled": true,
            "value": 1,
        }
    ))
    .is_err();

    assert!(rejects_scalar_value);

    let rejects_array_value = serde_json::from_value::<FeatureConfig>(json!(
        {
            "featureId": "some_control",
            "enabled": true,
            "value": [1, 2, 3],
        }
    ))
    .is_err();

    assert!(rejects_array_value);

    Ok(())
}
