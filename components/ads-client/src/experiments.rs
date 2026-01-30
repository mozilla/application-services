/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Nimbus experiment integration for ads-client.

use nimbus::NimbusClient;
use std::sync::Arc;

const FEATURE_ID: &str = "ads-client";

/// Query NimbusClient to check if HTTP cache is enabled for this user.
/// Returns true (cache enabled) by default if no NimbusClient or no experiment is active.
pub fn is_http_cache_enabled(nimbus: Option<Arc<NimbusClient>>) -> bool {
    let Some(nimbus) = nimbus else {
        return true; // Default: cache enabled
    };

    let Ok(Some(json)) = nimbus.get_feature_config_variables(FEATURE_ID.to_string()) else {
        return true; // Default: cache enabled
    };

    serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| v.get("http-cache-enabled")?.as_bool())
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nimbus::AppContext;
    use serde_json::json;
    use uuid::Uuid;

    struct NoopMetricsHandler;
    impl nimbus::metrics::MetricsHandler for NoopMetricsHandler {
        fn record_enrollment_statuses(&self, _: Vec<nimbus::metrics::EnrollmentStatusExtraDef>) {}
        fn record_feature_activation(&self, _: nimbus::metrics::FeatureExposureExtraDef) {}
        fn record_feature_exposure(&self, _: nimbus::metrics::FeatureExposureExtraDef) {}
        fn record_malformed_feature_config(
            &self,
            _: nimbus::metrics::MalformedFeatureConfigExtraDef,
        ) {
        }
    }

    #[test]
    fn test_fifty_fifty_experiment() {
        let experiment = json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "ads-cache-test",
                "featureIds": ["ads-client"],
                "branches": [
                    { "slug": "control", "ratio": 1, "feature": { "featureId": "ads-client", "value": { "http-cache-enabled": true } } },
                    { "slug": "treatment", "ratio": 1, "feature": { "featureId": "ads-client", "value": { "http-cache-enabled": false } } }
                ],
                "bucketConfig": { "count": 10000, "start": 0, "total": 10000, "namespace": "test", "randomizationUnit": "nimbus_id" },
                "appName": "test-app", "appId": "org.mozilla.test", "channel": "test",
                "userFacingName": "Test", "userFacingDescription": "Test",
                "isEnrollmentPaused": false, "proposedEnrollment": 7, "referenceBranch": "control"
            }]
        }).to_string();

        let (mut enabled, mut disabled) = (0, 0);
        for i in 0..100 {
            let tmp_dir = tempfile::tempdir().unwrap();
            let nimbus = Arc::new(
                NimbusClient::new(
                    AppContext {
                        app_name: "test-app".into(),
                        app_id: "org.mozilla.test".into(),
                        channel: "test".into(),
                        ..Default::default()
                    },
                    None,
                    vec![],
                    tmp_dir.path(),
                    Box::new(NoopMetricsHandler),
                    None,
                    None,
                    None,
                )
                .unwrap(),
            );
            nimbus.initialize().unwrap();
            // Create a deterministic UUID from the loop index
            let mut bytes = [0u8; 16];
            bytes[0] = i as u8;
            nimbus.set_nimbus_id(&Uuid::from_bytes(bytes)).unwrap();
            nimbus.set_experiments_locally(experiment.clone()).unwrap();
            nimbus.apply_pending_experiments().unwrap();

            if is_http_cache_enabled(Some(nimbus.clone())) {
                enabled += 1;
            } else {
                disabled += 1;
            }
        }

        assert!(
            (30..=70).contains(&enabled),
            "Expected ~50% enabled, got {enabled}/100"
        );
        assert!(
            (30..=70).contains(&disabled),
            "Expected ~50% disabled, got {disabled}/100"
        );
    }
}
