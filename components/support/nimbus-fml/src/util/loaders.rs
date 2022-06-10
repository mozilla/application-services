/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::{
    error::{FMLError, Result},
    SUPPORT_URL_LOADING,
};

use reqwest::blocking::{Client, ClientBuilder};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap},
    env,
    fmt::Display,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use url::Url;

pub(crate) const GITHUB_USER_CONTENT_DOTCOM: &str = "https://raw.githubusercontent.com";

/// A small enum for working with URLs and relative files
#[derive(PartialEq, Debug)]
pub enum FilePath {
    Local(PathBuf),
    Remote(Url),
}

impl FilePath {
    /// Appends a suffix to a path.
    /// If the `self` is a local file and the suffix is an absolute URL,
    /// then the return is the URL.
    pub fn join(&self, file: &str) -> Result<Self> {
        if file.contains("://") {
            return Ok(FilePath::Remote(Url::parse(file)?));
        }
        Ok(match self {
            FilePath::Local(p) => Self::Local(
                p.parent()
                    .expect("a file within a parent directory")
                    .join(file),
            ),
            FilePath::Remote(u) => Self::Remote(u.join(file)?),
        })
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FilePath::Local(p) => p.display().to_string(),
                FilePath::Remote(u) => u.to_string(),
            }
        )
    }
}

impl From<&Path> for FilePath {
    fn from(path: &Path) -> Self {
        Self::Local(path.into())
    }
}

static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

/// Utility class to abstract away the differences between loading from file and network.
///
/// With a nod to offline developer experience, files which come from the network
/// are cached on disk.
///
/// The cache directory should be in a directory that will get purged on a clean build.
///
/// This allows us to import files from another repository (via https) or include files
/// from a local files.
///
/// The loader is able to resolve a shortcut syntax similar to other package managers.
///
/// By default a prefix of `@XXXX/YYYY`: resolves to the `main` branch `XXXX/YYYY` Github repo.
///
/// The config is a map of repository names to paths, URLs or branches.
pub struct FileLoader {
    cache_dir: PathBuf,
    fetch_client: Client,

    config: BTreeMap<String, String>,
}

impl FileLoader {
    pub fn new(cache_dir: PathBuf, config: BTreeMap<String, String>) -> Result<Self> {
        if cache_dir.exists() {
            if !cache_dir.is_dir() {
                return Err(FMLError::InvalidPath(format!(
                    "Cache directory exists and is not a directory: {:?}",
                    cache_dir
                )));
            }
        } else {
            std::fs::create_dir_all(cache_dir.as_path())?;
        }

        let http_client = ClientBuilder::new()
            .https_only(true)
            .user_agent(USER_AGENT)
            .build()?;

        Ok(Self {
            cache_dir,
            fetch_client: http_client,

            config,
        })
    }

    pub fn default() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let cache_path = cwd.join("build/app/fml-cache");
        Self::new(cache_path, Default::default())
    }

    /// This loads a text file from disk or the network.
    ///
    /// If it's coming from the network, then cache the file to disk (based on the URL).
    ///
    /// We don't worry about cache invalidation, because a clean build should blow the cache
    /// away.
    pub fn read_to_string(&self, file: &FilePath) -> Result<String> {
        Ok(match file {
            FilePath::Local(path) => std::fs::read_to_string(path)?,
            FilePath::Remote(url) => self.fetch_and_cache(url)?,
        })
    }

    fn fetch_and_cache(&self, url: &Url) -> Result<String> {
        if !SUPPORT_URL_LOADING {
            unimplemented!("Loading manifests from URLs is not yet supported ({})", url);
        }
        let path_buf = self.create_cache_path_buf(url);
        Ok(if path_buf.exists() {
            std::fs::read_to_string(path_buf)?
        } else {
            let res = self.fetch_client.get(url.clone()).send()?;
            let text = res.text()?;

            std::fs::write(path_buf, &text)?;
            text
        })
    }

    fn create_cache_path_buf(&self, url: &Url) -> PathBuf {
        // Method to look after the cache directory.
        // We can organize this how we want: in this case we use a flat structure
        // with a hash of the URL as a prefix of the directory.
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        let checksum = hasher.finish();
        let filename = match url.path_segments() {
            Some(segments) => segments.last().unwrap_or("unknown.txt"),
            None => "unknown.txt",
        };
        // Take the last 16 bytes of the hash to make sure our prefixes are still random, but
        // not crazily long.
        let filename = format!("{:x}_{}", (checksum & 0x000000000000FFFF) as u16, filename,);
        self.cache_dir.join(filename)
    }

    /// Joins a path to a string, to make a new path.
    ///
    /// We want to be able to support local and remote files.
    /// We also want to be able to support a configurable short cut format.
    /// Following a pattern common in other package managers, `@XXXX/YYYY`
    /// is used as short hand for the main branch in github repos.
    pub fn join(&self, base: &FilePath, f: &str) -> Result<FilePath> {
        if f.starts_with('@') {
            let f = f.replacen('@', "", 1);
            let parts = f.splitn(3, '/').collect::<Vec<&str>>();
            match parts.as_slice() {
                [user, repo, path] => {
                    let repo = self.lookup_repo_path(user, repo);
                    let f = format!("{}/{}", repo, path);
                    base.join(&f)
                }
                _ => Err(FMLError::InvalidPath(format!(
                    "'{}' needs to include a username, a repo and a filepath",
                    f
                ))),
            }
        } else {
            base.join(f)
        }
    }

    fn lookup_repo_path(&self, user: &str, repo: &str) -> String {
        let default = "main".to_string();

        let value = self
            .config
            .get(&format!("{}/{}", user, repo))
            .or_else(|| self.config.get(&format!("@{}/{}", user, repo)))
            .unwrap_or(&default);

        if value.contains("://") || value.starts_with('.') {
            // It's a URL, or a relative path!
            value.clone()
        } else {
            // It's a branch.
            let branch = value;
            [GITHUB_USER_CONTENT_DOTCOM, user, repo, branch].join("/")
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_relative_paths() -> Result<()> {
        let tmp = std::env::temp_dir();

        let file = tmp.join("foo/bar.txt");
        let obs = FilePath::from(file.as_path());

        assert!(matches!(obs, FilePath::Local(_)));
        assert!(obs.to_string().ends_with("foo/bar.txt"));

        let obs = obs.join("baz.txt")?;
        assert!(obs.to_string().ends_with("foo/baz.txt"));

        let obs = obs.join("./bam.txt")?;
        // We'd prefer it to be like this:
        // assert!(obs.to_string().ends_with("foo/bam.txt"));
        // But there's no easy way to get this (because symlinks).
        // This is most likely the correct thing for us to do.
        // We put this test here for documentation purposes, and to
        // highlight that with URLs, ../ and ./ do what you might
        // expect.
        assert!(obs.to_string().ends_with("foo/./bam.txt"));

        let obs = obs.join("https://example.com/foo/bar.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(obs.to_string(), "https://example.com/foo/bar.txt");

        let obs = obs.join("baz.txt")?;
        assert_eq!(obs.to_string(), "https://example.com/foo/baz.txt");

        let obs = obs.join("./bam.txt")?;
        assert_eq!(obs.to_string(), "https://example.com/foo/bam.txt");

        let obs = obs.join("../brum/bram.txt")?;
        assert_eq!(obs.to_string(), "https://example.com/brum/bram.txt");

        Ok(())
    }

    #[test]
    fn test_at_shorthand_with_config() -> Result<()> {
        let tmp = std::env::temp_dir();

        let fp = tmp.join("base/old.txt");
        let fp = FilePath::from(fp.as_path());

        let mut config = BTreeMap::new();
        config.insert(
            "@repos/url".to_string(),
            "https://example.com/remote/directory/path".to_string(),
        );
        config.insert("@repos/branch".to_string(), "develop".to_string());
        config.insert("@repos/local".to_string(), "../directory/path".to_string());

        let loader = FileLoader::new(tmp, config)?;

        let obs = loader.join(&fp, "a/file.txt")?;
        assert!(matches!(obs, FilePath::Local(_)));
        assert!(obs.to_string().ends_with("base/a/file.txt"));

        let obs = loader.join(&fp, "@mozilla/application-services/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://raw.githubusercontent.com/mozilla/application-services/main/a/file.txt"
        );

        let obs = loader.join(&fp, "@repos/url/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://example.com/remote/directory/path/a/file.txt"
        );

        let obs = loader.join(&fp, "@repos/branch/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://raw.githubusercontent.com/repos/branch/develop/a/file.txt"
        );

        let obs = loader.join(&fp, "@repos/local/a/file.txt")?;
        assert!(matches!(obs, FilePath::Local(_)));
        assert!(obs
            .to_string()
            .ends_with("base/../directory/path/a/file.txt"));

        Ok(())
    }
}
