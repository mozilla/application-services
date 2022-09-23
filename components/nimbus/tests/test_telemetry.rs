/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod common;
#[cfg(all(feature = "rkv-safe-mode", feature = "builtin-glean"))]
#[cfg(test)]
mod test {

    use super::common::new_test_client_with_db;

    #[cfg(feature = "builtin-glean")]
    use glean::{test_reset_glean, ClientInfoMetrics, Configuration, RecordedEvent};
    #[cfg(feature = "builtin-glean")]
    use nimbus::glean_metrics;

    use nimbus::error::Result;
    use serde_json::json;

    #[test]
    fn test_enrollment_telemetry() -> Result<()> {
        // First, a little setup with a matching and non-matching experiment for testing
        // the enrollment telemetry
        let temp_dir = tempfile::tempdir()?;
        let client = new_test_client_with_db(&temp_dir)?;
        client.initialize()?;
        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "false",
                "userFacingName": "Diagnostic test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for diagnostic purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            },
            {
                "schemaVersion": "1.0.0",
                "slug": "secure-silver",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-silver",
                    "randomizationUnit": "nimbus_id"
                },
                "userFacingName": "Diagnostic test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for diagnostic purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }
            ]
        }))?;

        let tmp_glean_dir = temp_dir.path().join("glean_data");
        test_reset_glean(
            Configuration {
                data_path: tmp_glean_dir,
                application_id: "org.mozilla.fenix".into(),
                upload_enabled: true,
                max_events: None,
                delay_ping_lifetime_io: false,
                server_endpoint: Some("invalid-test-host".into()),
                uploader: None,
                use_core_mps: false,
            },
            ClientInfoMetrics::unknown(),
            false,
        );

        client.set_experiments_locally(experiment_json)?;
        let change_events = client.apply_pending_experiments()?;

        // Check for Glean enrollment event
        let events: Option<Vec<RecordedEvent>> =
            glean_metrics::nimbus_events::enrollment.test_get_value("events");
        assert!(events.is_some());
        assert_eq!(1, events.clone().unwrap().len());
        let event_extra = events.unwrap()[0].extra.to_owned().unwrap();
        assert_eq!("secure-silver", event_extra["experiment"]);
        assert_eq!(change_events[0].branch_slug, event_extra["branch"]);
        assert_eq!(change_events[0].enrollment_id, event_extra["enrollment_id"]);

        let experiment_json = serde_json::to_string(&json!({
            "data": [{
                "schemaVersion": "1.0.0",
                "slug": "secure-gold",
                "endDate": null,
                "featureIds": ["some-feature"],
                "branches": [
                    {
                    "slug": "control",
                    "ratio": 1
                    },
                    {
                    "slug": "treatment",
                    "ratio": 1
                    }
                ],
                "channel": "nightly",
                "probeSets": [],
                "startDate": null,
                "appName": "fenix",
                "appId": "org.mozilla.fenix",
                "bucketConfig": {
                    "count": 10000,
                    "start": 0,
                    "total": 10000,
                    "namespace": "secure-gold",
                    "randomizationUnit": "nimbus_id"
                },
                "targeting": "false",
                "userFacingName": "Diagnostic test experiment",
                "referenceBranch": "control",
                "isEnrollmentPaused": false,
                "proposedEnrollment": 7,
                "userFacingDescription": "This is a test experiment for diagnostic purposes.",
                "id": "secure-copper",
                "last_modified": 1_602_197_324_372i64,
            }]
        }))?;

        // Drop the NimbusClient to terminate the underlying database connection and restart
        drop(client);
        let client = new_test_client_with_db(&temp_dir)?;
        client.initialize()?;
        client.set_experiments_locally(experiment_json)?;
        let change_events = client.apply_pending_experiments()?;

        // Check for Glean unenrollment event with the correct `reason`
        let events: Option<Vec<RecordedEvent>> =
            glean_metrics::nimbus_events::unenrollment.test_get_value("events");
        assert!(events.is_some());
        assert_eq!(1, events.clone().unwrap().len());
        let event_extra = events.unwrap()[0].extra.to_owned().unwrap();
        assert_eq!("secure-silver", event_extra["experiment"]);
        assert_eq!(change_events[0].branch_slug, event_extra["branch"]);
        assert_eq!(change_events[0].enrollment_id, event_extra["enrollment_id"]);

        // Now let's opt into the secure-gold experiment and check for the enrollment event
        let change_events =
            client.opt_in_with_branch("secure-gold".to_string(), "control".to_string())?;

        // Check for Glean enrollment event
        let events: Option<Vec<RecordedEvent>> =
            glean_metrics::nimbus_events::enrollment.test_get_value("events");
        assert!(events.is_some());
        // This is the second enrollment. Because we didn't reset Glean and didn't send an
        // events ping yet, the enrollment from above should still be there.
        assert_eq!(2, events.clone().unwrap().len());
        let event_extra = events.unwrap()[1].extra.to_owned().unwrap();
        assert_eq!("secure-gold", event_extra["experiment"]);
        assert_eq!(change_events[0].branch_slug, event_extra["branch"]);
        assert_eq!(change_events[0].enrollment_id, event_extra["enrollment_id"]);

        // Next we will opt out of the secure-gold experiment and check for unenrollment telemetry
        let change_events = client.opt_out("secure-gold".to_string())?;

        // Check for Glean disqualification event with the correct `reason` of opt-out
        let events: Option<Vec<RecordedEvent>> =
            glean_metrics::nimbus_events::disqualification.test_get_value("events");
        assert!(events.is_some());
        assert_eq!(1, events.clone().unwrap().len());
        let event_extra = events.unwrap()[0].extra.to_owned().unwrap();
        assert_eq!("secure-gold", event_extra["experiment"]);
        assert_eq!(change_events[0].branch_slug, event_extra["branch"]);
        assert_eq!(change_events[0].enrollment_id, event_extra["enrollment_id"]);
        assert_eq!(
            *change_events[0].reason.as_ref().unwrap(),
            event_extra["reason"]
        );
        Ok(())
    }
}
