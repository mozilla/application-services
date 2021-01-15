/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Testing the two phase updates.
// This test crashes lmdb for reasons that make no sense, so only run it
// in the "safe mode" backend.

mod common;

#[cfg(feature = "rkv-safe-mode")]
#[cfg(test)]
mod test {
    use super::common::{initial_test_experiments, new_test_client, no_test_experiments};
    #[cfg(feature = "rkv-safe-mode")]
    use nimbus::{error::Result, NimbusClient};

    fn startup(client: &NimbusClient, first_run: bool) -> Result<()> {
        if first_run {
            client.set_experiments_locally(initial_test_experiments())?;
        }
        client.apply_pending_experiments()?;
        client.fetch_experiments()?;
        Ok(())
    }

    #[test]
    fn test_two_phase_update() -> Result<()> {
        let client = new_test_client("test_two_phase_update")?;
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
        let client = new_test_client("test_set_experiments_locally")?;
        assert_experiment_count(&client, 0)?;

        client.set_experiments_locally(initial_test_experiments())?;
        assert_experiment_count(&client, 0)?;

        client.apply_pending_experiments()?;
        assert_experiment_count(&client, 2)?;

        client.set_experiments_locally(no_test_experiments())?;
        assert_experiment_count(&client, 2)?;

        client.apply_pending_experiments()?;
        assert_experiment_count(&client, 0)?;

        Ok(())
    }

    #[cfg(feature = "rkv-safe-mode")]
    #[test]
    fn test_startup_behavior() -> Result<()> {
        let client = new_test_client("test_startup_behavior")?;
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
