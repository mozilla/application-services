use anyhow::{Context, Result};
use std::path::PathBuf;

pub(crate) struct GitOps {
    pub(crate) root_path: PathBuf,
}

impl GitOps {
    pub(crate) fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    pub(crate) fn create_branch(&self, name: &str) -> Result<()> {
        let status = std::process::Command::new("git")
            .args(["checkout", "-b", name])
            .current_dir(&self.root_path)
            .status()
            .context("Failed to create branch")?;

        if !status.success() {
            anyhow::bail!("Failed to create branch");
        }

        Ok(())
    }

    pub(crate) fn commit_changes(&self) -> Result<()> {
        let status = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&self.root_path)
            .status()
            .context("Failed to stage changes")?;

        if !status.success() {
            anyhow::bail!("Failed to stage changes");
        }

        let status = std::process::Command::new("git")
            .args([
                "commit",
                "-m", "Update Remote Settings defaults\n\nAutomated update of Remote Settings default values"
            ])
            .current_dir(&self.root_path)
            .status()
            .context("Failed to commit changes")?;

        if !status.success() {
            anyhow::bail!("Failed to commit changes");
        }

        Ok(())
    }

    pub(crate) fn push_branch(&self, name: &str) -> Result<()> {
        let status = std::process::Command::new("git")
            .args(["push", "origin", name])
            .current_dir(&self.root_path)
            .status()
            .context("Failed to push branch")?;

        if !status.success() {
            anyhow::bail!("Failed to push branch");
        }

        println!("Branch '{}' has been pushed to origin.", name);
        println!(
            "You can create a PR at: https://github.com/mozilla/application-services/pull/new/{}",
            name
        );

        Ok(())
    }
}
