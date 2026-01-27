/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Nimbus experiment integration for ads-client.

use nimbus::{AppContext, NimbusClient};
use serde_json::json;
use std::sync::Arc;

const FEATURE_ID: &str = "ads-client";

struct NoopMetricsHandler;
impl nimbus::metrics::MetricsHandler for NoopMetricsHandler {
    fn record_enrollment_statuses(&self, _: Vec<nimbus::metrics::EnrollmentStatusExtraDef>) {}
    fn record_feature_activation(&self, _: nimbus::metrics::FeatureExposureExtraDef) {}
    fn record_feature_exposure(&self, _: nimbus::metrics::FeatureExposureExtraDef) {}
    fn record_malformed_feature_config(&self, _: nimbus::metrics::MalformedFeatureConfigExtraDef) {}
}

/// Internal experiment client that manages a NimbusClient with hardcoded experiment.
pub struct ExperimentClient {
    nimbus: Arc<NimbusClient>,
}

impl ExperimentClient {
    /// Create a new ExperimentClient with hardcoded 50/50 experiment.
    pub fn new(db_path: &str) -> Option<Self> {
        let ctx = AppContext {
            app_name: "ads-client".into(),
            app_id: "org.mozilla.ads-client".into(),
            channel: "release".into(),
            ..Default::default()
        };

        // For a real implementation, the app would typically:
        // 1. Pass its own `AppContext` with real app info
        // 2. Pass a `MetricsHandler` that records to Glean
        // 3. Configure remote settings to fetch experiments from Mozilla's experiment server
        // 4. Not hardcode experiments in code
        let nimbus = Arc::new(
            NimbusClient::new(
                ctx,
                None,
                vec![],
                db_path,
                Box::new(NoopMetricsHandler),
                None,
                None,
                None,
            )
            .ok()?,
        );

        nimbus.initialize().ok()?;

        // Hardcoded 50/50 experiment for http-cache-enabled
        let experiment = json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "ads-client-cache",
                "featureIds": ["ads-client"],
                "branches": [
                    { "slug": "control", "ratio": 1, "feature": { "featureId": "ads-client", "value": { "http-cache-enabled": true } } },
                    { "slug": "treatment", "ratio": 1, "feature": { "featureId": "ads-client", "value": { "http-cache-enabled": false } } }
                ],
                "bucketConfig": { "count": 10000, "start": 0, "total": 10000, "namespace": "ads-client-cache", "randomizationUnit": "nimbus_id" },
                "appName": "ads-client", "appId": "org.mozilla.ads-client", "channel": "release",
                "userFacingName": "Ads Client Cache", "userFacingDescription": "50/50 experiment for HTTP cache",
                "isEnrollmentPaused": false, "proposedEnrollment": 7, "referenceBranch": "control"
            }]
        }).to_string();

        nimbus.set_experiments_locally(experiment).ok()?;
        nimbus.apply_pending_experiments().ok()?;

        Some(Self { nimbus })
    }

    /// Check if HTTP cache is enabled for this user.
    pub fn is_http_cache_enabled(&self) -> bool {
        let Ok(Some(json)) = self
            .nimbus
            .get_feature_config_variables(FEATURE_ID.to_string())
        else {
            return true;
        };

        serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v.get("http-cache-enabled")?.as_bool())
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifty_fifty_experiment() {
        let (mut enabled, mut disabled) = (0, 0);
        for i in 0..100 {
            let tmp_dir = tempfile::tempdir().unwrap();
            let db_path = tmp_dir.path().join("nimbus.db");
            let client = ExperimentClient::new(db_path.to_str().unwrap()).unwrap();

            // Set a unique nimbus_id for each "user"
            let mut bytes = [0u8; 16];
            bytes[0] = i as u8;
            client
                .nimbus
                .set_nimbus_id(&uuid::Uuid::from_bytes(bytes))
                .unwrap();
            client.nimbus.apply_pending_experiments().unwrap();

            if client.is_http_cache_enabled() {
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
