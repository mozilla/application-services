/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod common;
#[cfg(feature = "rkv-safe-mode")]
#[cfg(test)]
mod test {

    use super::common::new_test_client_with_db;
    #[cfg(feature = "rkv-safe-mode")]
    use nimbus::error::Result;
    use serde_json::json;

    #[test]
    fn test_restart_opt_in() -> Result<()> {
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
                    "count": 0,
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
        client.set_experiments_locally(experiment_json.clone())?;
        client.apply_pending_experiments()?;
        // the secure-gold experiment has a 'targeting' of "false", we test to ensure that
        // restarting the app preserves the fact that we opt-ed in, even though we were not
        // targeted
        client.opt_in_with_branch("secure-gold".into(), "treatment".into())?;
        // the secure-silver experiment has a bucket configuration of 0%, meaning we will always not
        // be enrolled, we test to ensure that is overridden when we opt-in
        client.opt_in_with_branch("secure-silver".into(), "treatment".into())?;

        let before_restart_experiments = client.get_active_experiments()?;
        assert_eq!(before_restart_experiments.len(), 2);
        assert_eq!(before_restart_experiments[0].branch_slug, "treatment");
        assert_eq!(before_restart_experiments[1].branch_slug, "treatment");
        // we drop the NimbusClient to terminate the underlying database connection
        drop(client);

        let client = new_test_client_with_db(&temp_dir)?;
        client.initialize()?;
        client.set_experiments_locally(experiment_json)?;
        client.apply_pending_experiments()?;
        let after_restart_experiments = client.get_active_experiments()?;
        assert_eq!(
            before_restart_experiments.len(),
            after_restart_experiments.len()
        );
        assert_eq!(after_restart_experiments[0].branch_slug, "treatment");
        assert_eq!(after_restart_experiments[1].branch_slug, "treatment");

        Ok(())
    }
}
