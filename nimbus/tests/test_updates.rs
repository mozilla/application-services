/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing the two phase updates.
// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.
#[cfg(feature = "rkv-safe-mode")]
#[cfg(test)]
mod test {
    #[cfg(feature = "rkv-safe-mode")]
    use nimbus::{error::Result, AppContext, NimbusClient, RemoteSettingsConfig};

    fn initial_experiments() -> String {
        use serde_json::json;
        json!({
            "data": [
                {
                    "schemaVersion": "1.0.0",
                    "slug": "startup-gold",
                    "endDate": null,
                    "branches":[
                        {"slug": "control", "ratio": 1},
                        {"slug": "treatment","ratio":1}
                    ],
                    "probeSets":[],
                    "startDate":null,
                    "application":"fenix",
                    "bucketConfig":{
                        // Setup to enroll everyone by default.
                        "count":10_000,
                        "start":0,
                        "total":10_000,
                        "namespace":"startup-gold",
                        "randomizationUnit":"nimbus_id"
                    },
                    "userFacingName":"Diagnostic test experiment",
                    "referenceBranch":"control",
                    "isEnrollmentPaused":false,
                    "proposedEnrollment":7,
                    "userFacingDescription":"This is a test experiment for diagnostic purposes.",
                    "id":"secure-gold",
                    "last_modified":1_602_197_324_372i64
                },
                {
                    "schemaVersion": "1.0.0",
                    "slug": "secure-gold",
                    "endDate": null,
                    "branches":[
                        {"slug": "control", "ratio": 1},
                        {"slug": "treatment","ratio":1}
                    ],
                    "probeSets":[],
                    "startDate":null,
                    "application":"fenix",
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
                }
            ]
        })
        .to_string()
    }

    fn no_experiments() -> String {
        use serde_json::json;
        json!({
            "data": []
        })
        .to_string()
    }

    fn new_client(identifier: &str) -> Result<NimbusClient> {
        use std::path::PathBuf;
        use tempdir::TempDir;
        use url::Url;
        let _ = env_logger::try_init();
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        dir.push("tests/experiments");
        let tmp_dir = TempDir::new(identifier)?;
        let url = Url::from_file_path(dir).expect("experiments dir should exist");

        let config = RemoteSettingsConfig {
            server_url: url.as_str().to_string(),
            bucket_name: "doesn't matter".to_string(),
            collection_name: "doesn't matter".to_string(),
        };
        let aru = Default::default();
        let ctx = AppContext {
            app_id: "fenix".to_string(),
            ..Default::default()
        };
        NimbusClient::new(ctx, tmp_dir.path(), Some(config), aru)
    }

    fn startup(client: &NimbusClient, first_run: bool) -> Result<()> {
        if first_run {
            client.set_experiments_locally(initial_experiments())?;
        }
        client.apply_pending_experiments()?;
        client.fetch_experiments()?;
        Ok(())
    }

    #[test]
    fn test_two_phase_update() -> Result<()> {
        let client = new_client("test_two_phase_update")?;
        client.fetch_experiments()?;

        // We have fetched the experiments from the server, but not put them into use yet.
        // The experiments are pending.
        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 0);

        // Now, the app chooses when to apply the pending updates to the experiments.
        let events: Vec<_> = client.apply_pending_experiments()?;
        assert_eq!(events.len(), 1);

        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 1);
        assert_eq!(experiments[0].slug, "secure-gold");

        // Next time we start the app, we immediately apply pending updates,
        // but there may not be any waiting.
        let events: Vec<_> = client.apply_pending_experiments()?;
        // No change events
        assert_eq!(events.len(), 0);

        // Confirm that nothing has changed.
        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 1);
        assert_eq!(experiments[0].slug, "secure-gold");

        Ok(())
    }

    fn assert_experiment_count(client: &NimbusClient, count: usize) -> Result<()> {
        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), count);
        Ok(())
    }

    #[cfg(feature = "rkv-safe-mode")]
    #[test]
    fn test_set_experiments_locally() -> Result<()> {
        let client = new_client("test_set_experiments_locally")?;
        assert_experiment_count(&client, 0)?;

        client.set_experiments_locally(initial_experiments())?;
        assert_experiment_count(&client, 0)?;

        client.apply_pending_experiments()?;
        assert_experiment_count(&client, 2)?;

        client.set_experiments_locally(no_experiments())?;
        assert_experiment_count(&client, 2)?;

        client.apply_pending_experiments()?;
        assert_experiment_count(&client, 0)?;

        Ok(())
    }

    #[cfg(feature = "rkv-safe-mode")]
    #[test]
    fn test_startup_behavior() -> Result<()> {
        let client = new_client("test_startup_behavior")?;
        startup(&client, true)?;

        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 2);
        assert_eq!(experiments[0].slug, "secure-gold");
        assert_eq!(experiments[1].slug, "startup-gold");

        // The app is at a safe place to change all the experiments.
        client.apply_pending_experiments()?;
        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 1);
        assert_eq!(experiments[0].slug, "secure-gold");

        // Next time we start the app.
        startup(&client, false)?;
        let experiments = client.get_all_experiments()?;
        assert_eq!(experiments.len(), 1);
        assert_eq!(experiments[0].slug, "secure-gold");

        Ok(())
    }
}
