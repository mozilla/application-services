use crate::error::*;
use futures::{stream::FuturesUnordered, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::{path::PathBuf, sync::Arc};
use walkdir::WalkDir;

const DUMPS_DIR: &str = "dumps";

pub struct CollectionDownloader {
    client: reqwest::Client,
    multi_progress: Arc<MultiProgress>,
    output_dir: PathBuf,
}

#[derive(Deserialize, Serialize)]
pub struct CollectionData {
    data: Vec<Value>,
    timestamp: u64,
}

pub struct UpdateResult {
    updated: Vec<String>,
    up_to_date: Vec<String>,
    not_found: Vec<String>,
}

impl CollectionDownloader {
    pub fn new(root_path: PathBuf) -> Self {
        let output_dir = if root_path.ends_with("components/remote_settings") {
            root_path
        } else {
            root_path.join("components").join("remote_settings")
        };

        Self {
            client: reqwest::Client::new(),
            multi_progress: Arc::new(MultiProgress::new()),
            output_dir,
        }
    }

    pub async fn run(&self, dry_run: bool, create_pr: bool) -> Result<()> {
        if dry_run && create_pr {
            return Err(RemoteSettingsError::Git(
                "Cannot use --dry-run with --create-pr".to_string(),
            )
            .into());
        }

        let result = self.download_all().await?;

        if dry_run {
            println!("\nDry run summary:");
            println!("- Would update {} collections", result.updated.len());
            println!(
                "- {} collections already up to date",
                result.up_to_date.len()
            );
            println!(
                "- {} collections not found on remote",
                result.not_found.len()
            );
            return Ok(());
        }

        println!("\nExecution summary:");
        if !result.updated.is_empty() {
            println!("Updated collections:");
            for collection in &result.updated {
                println!("  - {}", collection);
            }
        }

        if !result.up_to_date.is_empty() {
            println!("Collections already up to date:");
            for collection in &result.up_to_date {
                println!("  - {}", collection);
            }
        }

        if !result.not_found.is_empty() {
            println!("Collections not found on remote:");
            for collection in &result.not_found {
                println!("  - {}", collection);
            }
        }

        if !result.updated.is_empty() && create_pr {
            self.create_pull_request()?;
        }

        Ok(())
    }

    fn create_pull_request(&self) -> Result<()> {
        let git_ops = crate::git::GitOps::new(
            self.output_dir
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf(),
        );

        let branch_name = "remote-settings-update-dumps";

        git_ops.create_branch(branch_name)?;
        git_ops.commit_changes()?;
        git_ops.push_branch(branch_name)?;
        Ok(())
    }

    fn scan_local_dumps(&self) -> Result<HashMap<String, (String, u64)>> {
        let mut collections = HashMap::new();
        let dumps_dir = self.output_dir.join(DUMPS_DIR);

        for entry in WalkDir::new(dumps_dir).min_depth(2).max_depth(2) {
            let entry = entry?;
            if entry.file_type().is_file()
                && entry.path().extension().map_or(false, |ext| ext == "json")
            {
                // Get bucket name from parent directory
                let bucket = entry
                    .path()
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| RemoteSettingsError::Path("Invalid bucket path".into()))?;

                // Get collection name from filename
                let collection_name = entry
                    .path()
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| RemoteSettingsError::Path("Invalid collection name".into()))?;

                // Read and parse the file to get timestamp
                let content = std::fs::read_to_string(entry.path())?;
                let data: serde_json::Value = serde_json::from_str(&content)?;
                let timestamp = data["timestamp"].as_u64().ok_or_else(|| {
                    RemoteSettingsError::Json(serde_json::Error::custom("No timestamp found"))
                })?;

                collections.insert(
                    format!("{}/{}", bucket, collection_name),
                    (bucket.to_string(), timestamp),
                );
            }
        }
        Ok(collections)
    }

    async fn fetch_timestamps(&self) -> Result<HashMap<String, u64>> {
        let monitor_url = format!(
            "{}/buckets/monitor/collections/changes/records",
            "https://firefox.settings.services.mozilla.com/v1"
        );
        let monitor_response: Value = self.client.get(&monitor_url).send().await?.json().await?;

        Ok(monitor_response["data"]
            .as_array()
            .ok_or_else(|| {
                RemoteSettingsError::Json(serde_json::Error::custom(
                    "No data array in monitor response",
                ))
            })?
            .iter()
            .filter_map(|record| {
                let bucket = record["bucket"].as_str()?;
                let collection_name = record["collection"].as_str()?;
                Some((
                    format!("{}/{}", bucket, collection_name),
                    record["last_modified"].as_u64()?,
                ))
            })
            .collect())
    }

    async fn fetch_collection(
        &self,
        collection_name: String,
        last_modified: u64,
        pb: ProgressBar,
    ) -> Result<(String, CollectionData)> {
        let parts: Vec<&str> = collection_name.split('/').collect();
        if parts.len() != 2 {
            return Err(RemoteSettingsError::Json(serde_json::Error::custom(
                "Invalid collection name format",
            ))
            .into());
        }
        let (bucket, name) = (parts[0], parts[1]);

        let url = format!(
            "{}/buckets/{}/collections/{}/changeset?_expected={}",
            "https://firefox.settings.services.mozilla.com/v1", bucket, name, last_modified
        );

        pb.set_message(format!("Downloading {}", name));

        let response = self.client.get(&url).send().await?;
        let changeset: Value = response.json().await?;

        let timestamp = changeset["timestamp"].as_u64().ok_or_else(|| {
            RemoteSettingsError::Json(serde_json::Error::custom("No timestamp in changeset"))
        })?;

        pb.finish_with_message(format!("Downloaded {}", name));

        Ok((
            collection_name,
            CollectionData {
                data: changeset["changes"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .to_vec(),
                timestamp,
            },
        ))
    }

    pub async fn download_all(&self) -> Result<UpdateResult> {
        std::fs::create_dir_all(self.output_dir.join(DUMPS_DIR))?;

        let local_collections = self.scan_local_dumps()?;
        if local_collections.is_empty() {
            println!(
                "No local collections found in {:?}",
                self.output_dir.join(DUMPS_DIR)
            );
            return Ok(UpdateResult {
                updated: vec![],
                up_to_date: vec![],
                not_found: vec![],
            });
        }

        let remote_timestamps = self.fetch_timestamps().await?;
        let mut futures = FuturesUnordered::new();
        let mut up_to_date = Vec::new();
        let mut not_found = Vec::new();

        // Only check collections we have locally
        for (collection_key, (_, local_timestamp)) in local_collections {
            let remote_timestamp = match remote_timestamps.get(&collection_key) {
                Some(&timestamp) => timestamp,
                None => {
                    println!("Warning: Collection {} not found on remote", collection_key);
                    not_found.push(collection_key);
                    continue;
                }
            };

            let pb = self.multi_progress.add(ProgressBar::new(100));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {msg}")
                    .unwrap(),
            );

            if local_timestamp >= remote_timestamp {
                println!("Collection {} is up to date", collection_key);
                up_to_date.push(collection_key);
                continue;
            }

            println!("Collection {} needs update", collection_key);
            futures.push(self.fetch_collection(collection_key.clone(), remote_timestamp, pb));
        }

        let mut updated = Vec::new();
        while let Some(result) = futures.next().await {
            let (collection, data) = result?;
            self.write_collection_file(&collection, &data)?;
            updated.push(collection);
        }

        Ok(UpdateResult {
            updated,
            up_to_date,
            not_found,
        })
    }

    pub async fn download_single(&self, bucket: &str, collection_name: &str) -> Result<()> {
        std::fs::create_dir_all(self.output_dir.join(DUMPS_DIR))?;

        let collection_key = format!("{}/{}", bucket, collection_name);
        let pb = self.multi_progress.add(ProgressBar::new(100));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {msg}")
                .unwrap(),
        );

        let (_, data) = self.fetch_collection(collection_key.clone(), 0, pb).await?;

        // Write to file
        self.write_collection_file(&collection_key, &data)?;

        println!(
            "Successfully downloaded collection to {:?}/dumps/{}/{}.json",
            self.output_dir, bucket, collection_name
        );

        Ok(())
    }

    fn write_collection_file(&self, collection: &str, data: &CollectionData) -> Result<()> {
        let parts: Vec<&str> = collection.split('/').collect();
        if parts.len() != 2 {
            return Err(RemoteSettingsError::Path("Invalid collection path".into()).into());
        }
        let (bucket, name) = (parts[0], parts[1]);

        // Write to dumps directory
        let dumps_path = self
            .output_dir
            .join(DUMPS_DIR)
            .join(bucket)
            .join(format!("{}.json", name));

        std::fs::create_dir_all(dumps_path.parent().unwrap())?;
        std::fs::write(&dumps_path, serde_json::to_string_pretty(&data)?)?;

        Ok(())
    }
}
