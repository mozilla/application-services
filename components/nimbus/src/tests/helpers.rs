/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{AppContext, Experiment, NimbusTargetingHelper, TargetingAttributes};
use serde_json::json;

cfg_if::cfg_if! {
    if #[cfg(feature = "stateful")] {
        use crate::behavior::EventStore;
        use std::sync::{Arc, Mutex};
    }
}

impl From<TargetingAttributes> for NimbusTargetingHelper {
    fn from(value: TargetingAttributes) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                let store = Arc::new(Mutex::new(EventStore::new()));
                NimbusTargetingHelper::new(value, store)
            } else {
                NimbusTargetingHelper::new(value)
            }
        }
    }
}

impl Default for NimbusTargetingHelper {
    fn default() -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "stateful")] {
                let ctx: AppContext = Default::default();
                let store = Arc::new(Mutex::new(EventStore::new()));
                NimbusTargetingHelper::new(ctx, store)
            } else {
                let ctx: AppContext = Default::default();
                NimbusTargetingHelper::new(ctx)
            }
        }
    }
}

pub(crate) fn get_test_experiments() -> Vec<Experiment> {
    vec![
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-gold",
            "endDate": null,
            "featureIds": ["some_control"],
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": false,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        }
                    }
                },
                {
                    "slug": "treatment",
                    "ratio":1,
                    "feature": {
                        "featureId": "some_control",
                        "enabled": true,
                        "value": {
                            "text": "OK then",
                            "number": 42
                        }
                    }
                }
            ],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName": "fenix",
            "appId": "org.mozilla.fenix",
            "bucketConfig":{
                // Setup to enroll everyone by default.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-gold",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"Diagnostic test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"This is a test experiment for diagnostic purposes.",
            "id":"secure-gold",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "schemaVersion": "1.0.0",
            "slug": "secure-silver",
            "endDate": null,
            "branches":[
                {
                    "slug": "control",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": true,
                    }
                },
                {
                    "slug": "treatment",
                    "ratio": 1,
                    "feature": {
                        "featureId": "about_welcome",
                        "enabled": false,
                    }
                },
            ],
            "featureIds": ["about_welcome"],
            "channel": "nightly",
            "probeSets":[],
            "startDate":null,
            "appName":"fenix",
            "appId":"org.mozilla.fenix",
            "bucketConfig":{
                // Also enroll everyone.
                "count":10_000,
                "start":0,
                "total":10_000,
                "namespace":"secure-silver",
                "randomizationUnit":"nimbus_id"
            },
            "userFacingName":"2nd test experiment",
            "referenceBranch":"control",
            "isEnrollmentPaused":false,
            "proposedEnrollment":7,
            "userFacingDescription":"2nd test experiment.",
            "id":"secure-silver",
            "last_modified":1_602_197_324_372i64
        }))
        .unwrap(),
    ]
}

pub fn get_ios_rollout_experiment() -> Experiment {
    serde_json::from_value(json!(
    {
      "appId": "org.mozilla.ios.Firefox",
      "appName": "firefox_ios",
      "application": "org.mozilla.ios.Firefox",
      "arguments": {},
      "branches": [
        {
          "feature": {
            "enabled": true,
            "featureId": "coordinators-refactor-feature",
            "value": {
              "enabled": true
            }
          },
          "ratio": 1,
          "slug": "control"
        }
      ],
      "bucketConfig": {
        "count": 10000,
        "namespace": "ios-coordinators-refactor-feature-release-no_targeting-rollout-1",
        "randomizationUnit": "nimbus_id",
        "start": 0,
        "total": 10000
      },
      "channel": "release",
      "endDate": null,
      "enrollmentEndDate": null,
      "featureIds": [
        "coordinators-refactor-feature"
      ],
      "featureValidationOptOut": false,
      "id": "ios-coordinators-rollout",
      "isEnrollmentPaused": false,
      "isRollout": true,
      "localizations": null,
      "outcomes": [],
      "probeSets": [],
      "proposedDuration": 28,
      "proposedEnrollment": 7,
      "referenceBranch": "control",
      "schemaVersion": "1.12.0",
      "slug": "ios-coordinators-rollout",
      "startDate": null,
      "targeting": "(app_version|versionCompare('114.!') >= 0)",
      "userFacingDescription": "Rollout of coordinators refactor.",
      "userFacingName": "iOS Coordinators Rollout"
    }))
    .unwrap()
}
