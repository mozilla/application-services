/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::{
    commands::LoaderConfig,
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
#[derive(Clone, Debug)]
pub enum FilePath {
    Local(PathBuf),
    Remote(Url),
}

impl FilePath {
    pub fn new(cwd: &Path, file: &str) -> Result<Self> {
        Ok(if file.contains("://") {
            FilePath::Remote(Url::parse(file)?)
        } else {
            FilePath::Local(cwd.join(file))
        })
    }

    /// Appends a suffix to a path.
    /// If the `self` is a local file and the suffix is an absolute URL,
    /// then the return is the URL.
    pub fn join(&self, file: &str) -> Result<Self> {
        if file.contains("://") {
            return Ok(FilePath::Remote(Url::parse(file)?));
        }
        Ok(match self {
            Self::Local(p) => Self::Local(
                // We implement a join similar to Url::join.
                // If the root is a directory, we append;
                // if not we take the parent, then append.
                if is_dir(p) {
                    p.join(file)
                } else {
                    p.parent()
                        .expect("a file within a parent directory")
                        .join(file)
                },
            ),
            Self::Remote(u) => Self::Remote(u.join(file)?),
        })
    }

    pub fn canonicalize(&self) -> Result<Self> {
        Ok(match self {
            Self::Local(p) => Self::Local(p.canonicalize().map_err(|e| {
                // We do this map_err here because the IO Error message that comes out of `canonicalize`
                // doesn't include the problematic file path.
                FMLError::InvalidPath(format!("{}: {}", e, p.as_path().display()))
            })?),
            Self::Remote(u) => Self::Remote(u.clone()),
        })
    }
}

impl Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Local(p) => p.display().to_string(),
                Self::Remote(u) => u.to_string(),
            }
        )
    }
}

impl From<&Path> for FilePath {
    fn from(path: &Path) -> Self {
        Self::Local(path.into())
    }
}

#[cfg(not(test))]
fn is_dir(path_buf: &Path) -> bool {
    path_buf.is_dir()
}

#[cfg(test)]
fn is_dir(path_buf: &Path) -> bool {
    path_buf.display().to_string().ends_with('/')
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
#[derive(Clone, Debug)]
pub struct FileLoader {
    cache_dir: PathBuf,
    fetch_client: Client,

    config: BTreeMap<String, FilePath>,

    // This is used for resolving relative paths when no other path
    // information is available.
    cwd: PathBuf,
}

impl TryFrom<&LoaderConfig> for FileLoader {
    type Error = FMLError;

    fn try_from(value: &LoaderConfig) -> Result<Self, Self::Error> {
        let cache_dir = value.cache_dir.clone();
        let cwd = value.cwd.clone();

        let mut files = Self::new(cwd, cache_dir, Default::default())?;

        for f in &value.repo_files {
            let path = files.file_path(f)?;
            files.add_repo_file(&path)?;
        }

        Ok(files)
    }
}

impl FileLoader {
    pub fn new(
        cwd: PathBuf,
        cache_dir: PathBuf,
        config: BTreeMap<String, FilePath>,
    ) -> Result<Self> {
        if cache_dir.exists() && !cache_dir.is_dir() {
            return Err(FMLError::InvalidPath(format!(
                "Cache directory exists and is not a directory: {:?}",
                cache_dir
            )));
        }

        let http_client = ClientBuilder::new()
            .https_only(true)
            .user_agent(USER_AGENT)
            .build()?;

        Ok(Self {
            cache_dir,
            fetch_client: http_client,
            cwd,

            config,
        })
    }

    #[cfg(test)]
    pub fn default() -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let cache_path = cwd.join("build/app/fml-cache");
        Self::new(
            cache_path,
            std::env::current_dir().expect("CWD not set"),
            Default::default(),
        )
    }

    pub fn add_repo_file(&mut self, file: &FilePath) -> Result<()> {
        let string = self.read_to_string(file)?;

        let config: BTreeMap<String, String> = if file.to_string().ends_with(".json") {
            serde_json::from_str(&string)?
        } else {
            serde_yaml::from_str(&string)?
        };

        for (k, v) in config {
            self.add_repo(file, k, v)?;
        }

        Ok(())
    }

    fn add_repo(&mut self, cwd: &FilePath, k: String, v: String) -> Result<(), FMLError> {
        let k = k.replacen('@', "", 1);
        let v = if v.ends_with('/') {
            v
        } else {
            format!("{}/", v)
        };
        let v =
            if v.starts_with('.') || v.starts_with('/') || v.contains(":\\") || v.contains("://") {
                cwd.join(&v)?
            } else {
                self.remote_file_path(&k, &v)?
            };
        self.config.insert(k, v);
        Ok(())
    }

    fn remote_file_path(&self, repo: &str, branch_or_tag: &str) -> Result<FilePath, FMLError> {
        let base_url = format!("{}/{}/{}", GITHUB_USER_CONTENT_DOTCOM, repo, branch_or_tag);
        Ok(FilePath::Remote(Url::parse(&base_url)?))
    }

    fn default_remote_path(&self, key: String) -> FilePath {
        self.remote_file_path(&key, "main/")
            .expect("main branch never fails")
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

            let parent = path_buf.parent().expect("Cache directory is specified");
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }

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
        Ok(if let Some(u) = self.resolve_url_shortcut(f)? {
            u
        } else {
            base.join(f)?
        })
    }

    pub fn file_path(&self, f: &str) -> Result<FilePath> {
        Ok(if let Some(u) = self.resolve_url_shortcut(f)? {
            u
        } else {
            FilePath::new(&self.cwd, f)?
        })
    }

    fn resolve_url_shortcut(&self, f: &str) -> Result<Option<FilePath>> {
        if f.starts_with('@') {
            let f = f.replacen('@', "", 1);
            let parts = f.splitn(3, '/').collect::<Vec<&str>>();
            match parts.as_slice() {
                [user, repo, path] => {
                    let key = format!("{}/{}", user, repo);
                    Ok(if let Some(repo) = self.lookup_repo_path(user, repo) {
                        Some(repo.join(path)?)
                    } else {
                        let repo = self.default_remote_path(key);
                        Some(repo.join(path)?)
                    })
                }
                _ => Err(FMLError::InvalidPath(format!(
                    "'{}' needs to include a username, a repo and a filepath",
                    f
                ))),
            }
        } else {
            Ok(None)
        }
    }

    fn lookup_repo_path(&self, user: &str, repo: &str) -> Option<&FilePath> {
        let key = format!("{}/{}", user, repo);
        self.config.get(&key)
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::util::pkg_dir;

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
    fn test_at_shorthand_with_no_at() -> Result<()> {
        let files = create_loader()?;
        let cwd = FilePath::Local(files.cwd.clone());
        let src_file = cwd.join("base/old.txt")?;

        // A source file asks for a destination file relative to it.
        let obs = files.join(&src_file, "a/file.txt")?;
        assert!(matches!(obs, FilePath::Local(_)));
        assert!(obs.to_string().ends_with("/base/a/file.txt"));
        assert_eq!(obs.to_string(), format!("{}base/a/file.txt", &cwd));
        Ok(())
    }

    #[test]
    fn test_at_shorthand_default_branch() -> Result<()> {
        let files = create_loader()?;
        let cwd = FilePath::Local(files.cwd.clone());
        let src_file = cwd.join("base/old.txt")?;

        // A source file asks for a file in another repo. We haven't any specific configuration
        // for this repo, so we default to the `main` branch.
        let obs = files.join(&src_file, "@repo/unspecified/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://raw.githubusercontent.com/repo/unspecified/main/a/file.txt"
        );
        Ok(())
    }

    #[test]
    fn test_at_shorthand_absolute_url() -> Result<()> {
        let mut files = create_loader()?;
        let cwd = FilePath::Local(files.cwd.clone());
        let src_file = cwd.join("base/old.txt")?;

        // A source file asks for a file in another repo. The loader uses an absolute
        // URL as the base URL.
        files.add_repo(
            &cwd,
            "@repos/url".to_string(),
            "https://example.com/remote/directory/path".to_string(),
        )?;

        let obs = files.join(&src_file, "@repos/url/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://example.com/remote/directory/path/a/file.txt"
        );

        let obs = files.file_path("@repos/url/b/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://example.com/remote/directory/path/b/file.txt"
        );
        Ok(())
    }

    #[test]
    fn test_at_shorthand_specified_branch() -> Result<()> {
        let mut files = create_loader()?;
        let cwd = FilePath::Local(files.cwd.clone());
        let src_file = cwd.join("base/old.txt")?;

        // A source file asks for a file in another repo. The loader uses the branch/tag/ref
        // specified.
        files.add_repo(&cwd, "@repos/branch".to_string(), "develop".to_string())?;
        let obs = files.join(&src_file, "@repos/branch/a/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://raw.githubusercontent.com/repos/branch/develop/a/file.txt"
        );

        let obs = files.file_path("@repos/branch/b/file.txt")?;
        assert!(matches!(obs, FilePath::Remote(_)));
        assert_eq!(
            obs.to_string(),
            "https://raw.githubusercontent.com/repos/branch/develop/b/file.txt"
        );
        Ok(())
    }

    #[test]
    fn test_at_shorthand_local_development() -> Result<()> {
        let mut files = create_loader()?;
        let cwd = FilePath::Local(files.cwd.clone());
        let src_file = cwd.join("base/old.txt")?;

        // A source file asks for a file in another repo. The loader is configured to
        // give a file in a directory on the local filesystem.
        let rel_dir = "../directory/path";
        files.add_repo(&cwd, "@repos/local".to_string(), rel_dir.to_string())?;

        let obs = files.join(&src_file, "@repos/local/a/file.txt")?;
        assert!(matches!(obs, FilePath::Local(_)));
        assert_eq!(obs.to_string(), format!("{}{}/a/file.txt", &cwd, rel_dir));

        let obs = files.file_path("@repos/local/b/file.txt")?;
        assert!(matches!(obs, FilePath::Local(_)));
        assert_eq!(obs.to_string(), format!("{}{}/b/file.txt", &cwd, rel_dir));

        Ok(())
    }

    fn create_loader() -> Result<FileLoader, FMLError> {
        let cache_dir = std::env::temp_dir();
        let config = Default::default();
        let cwd = cache_dir.clone();
        let loader = FileLoader::new(cwd, cache_dir, config)?;
        Ok(loader)
    }

    #[test]
    fn test_at_shorthand_from_config_file() -> Result<()> {
        let cwd = PathBuf::from(pkg_dir());
        let cache_dir = std::env::temp_dir();

        let config = &LoaderConfig {
            cwd,
            cache_dir,
            repo_files: vec![
                "fixtures/loaders/config_files/remote.json".to_string(),
                "fixtures/loaders/config_files/local.yaml".to_string(),
            ],
        };

        let files: FileLoader = config.try_into()?;
        let cwd = FilePath::Local(files.cwd.clone());

        // This is a remote repo, specified in remote.json.
        let tfr = files.file_path("@my/remote/file.txt")?;
        assert_eq!(
            tfr.to_string(),
            "https://example.com/repo/branch/file.txt".to_string()
        );

        // This is a local file, specified in local.yaml
        let tf1 = files.file_path("@test/nested1/test-file.txt")?;
        assert_eq!(
            tf1.to_string(),
            format!(
                "{}/fixtures/loaders/config_files/./nested-1/test-file.txt",
                &cwd
            )
        );

        // This is a remote repo, specified in remote.json, but overridden in local.yaml
        let tf2 = files.file_path("@test/nested2/test-file.txt")?;
        assert_eq!(
            tf2.to_string(),
            format!(
                "{}/fixtures/loaders/config_files/./nested-2/test-file.txt",
                &cwd
            )
        );

        let tf1 = files.read_to_string(&tf1)?;
        let tf2 = files.read_to_string(&tf2)?;

        assert_eq!("test-file/1".to_string(), tf1);
        assert_eq!("test-file/2".to_string(), tf2);

        Ok(())
    }
}
