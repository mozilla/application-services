/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use serde_json::json;

use crate::{FeatureConfig, error::Result};

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

#[cfg(feature = "stateful")]
mod stateful {
    use serde_json::json;

    use crate::stateful::firefox_labs::*;
    use crate::tests::helpers::get_firefox_lab;

    #[cfg(feature = "stateful")]
    #[test]
    fn test_get_firefox_labs_metadata() {
        assert_eq!(
            get_firefox_lab("slug")
                .get_firefox_labs_metadata(false)
                .unwrap(),
            FirefoxLabsMetadata {
                slug: "slug".into(),
                enrolled: false,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into()),
            }
        );

        assert_eq!(
            get_firefox_lab("slug")
                .get_firefox_labs_metadata(true)
                .unwrap(),
            FirefoxLabsMetadata {
                slug: "slug".into(),
                enrolled: true,
                requires_restart: false,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://example.com/#feedback".into()),
            }
        );

        assert_eq!(
            get_firefox_lab("slug")
                .patch(json!({
                    "firefoxLabsDescriptionLinks": null,
                    "requiresRestart": true,
                }))
                .get_firefox_labs_metadata(false)
                .unwrap(),
            FirefoxLabsMetadata {
                slug: "slug".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: None,
            }
        );

        assert_eq!(
            get_firefox_lab("slug")
                .patch(json!({
                    "firefoxLabsDescriptionLinks": {
                        "feedback": "https://feedback.example.com/",
                    },
                    "requiresRestart": true,
                }))
                .get_firefox_labs_metadata(false)
                .unwrap(),
            FirefoxLabsMetadata {
                slug: "slug".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: Some("https://feedback.example.com/".into()),
            }
        );

        assert_eq!(
            get_firefox_lab("slug")
                .patch(json!({
                    "firefoxLabsDescriptionLinks": {},
                    "requiresRestart": true,
                }))
                .get_firefox_labs_metadata(false)
                .unwrap(),
            FirefoxLabsMetadata {
                slug: "slug".into(),
                enrolled: false,
                requires_restart: true,
                title_string_id: "labs-title".into(),
                description_string_id: "labs-description".into(),
                feedback_url: None,
            }
        );

        // Requires isFirefoxLabsOptIn: true
        assert!(
            get_firefox_lab("slug")
                .patch(json!({ "isFirefoxLabsOptIn": false }))
                .get_firefox_labs_metadata(false)
                .is_none()
        );

        // Requires isRollout: true
        assert!(
            get_firefox_lab("slug")
                .patch(json!({ "isRollout": false }))
                .get_firefox_labs_metadata(false)
                .is_none()
        );

        // Requires firefoxLabsTitle
        assert!(
            get_firefox_lab("slug")
                .patch(json!({ "firefoxLabsTitle": null }))
                .get_firefox_labs_metadata(false)
                .is_none()
        );

        // Requires firefoxLabsDescription
        assert!(
            get_firefox_lab("slug")
                .patch(json!({ "firefoxLabsDescription": null }))
                .get_firefox_labs_metadata(false)
                .is_none()
        );
    }
}
