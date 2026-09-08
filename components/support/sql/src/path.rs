/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::path::{Path, PathBuf};

use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // This will happen if you provide something absurd like
    // "/" or "" as your database path. For more subtley broken paths,
    // we'll likely return an IoError.
    #[error("Illegal database path: {0:?}")]
    IllegalDatabasePath(PathBuf),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// `Path` is basically just a `str` with no validation, and so in practice it
/// could contain a file URL. Rusqlite takes advantage of this a bit, and says
/// `AsRef<Path>` but really means "anything sqlite can take as an argument".
///
/// Swift loves using file urls (the only support it has for file manipulation
/// is through file urls), so it's handy to support them if possible.
fn unurl_path(p: impl AsRef<Path>) -> PathBuf {
    p.as_ref()
        .to_str()
        .and_then(|s| Url::parse(s).ok())
        .and_then(|u| {
            if u.scheme() == "file" {
                u.to_file_path().ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| p.as_ref().to_owned())
}

#[cfg(not(target_os = "android"))]
fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

/// `std::fs::canonicalize` asks `realpath` to allocate the resolved path and
/// releases it with `free`. On Android those are not the same allocator when
/// the caller links against mozglue, because bionic keeps its internal
/// `malloc` calls to itself while `free` resolves to mozjemalloc. Passing a
/// `PATH_MAX` buffer keeps the result caller owned.
#[cfg(target_os = "android")]
fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    use std::ffi::{CStr, CString, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut buffer = [0 as libc::c_char; libc::PATH_MAX as usize];
    // SAFETY: `buffer` is the `PATH_MAX` bytes `realpath` requires, and `path`
    // is a valid NUL terminated string for the duration of the call.
    let resolved = unsafe { libc::realpath(path.as_ptr(), buffer.as_mut_ptr()) };
    if resolved.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: on success `realpath` returns `buffer`, NUL terminated.
    let bytes = unsafe { CStr::from_ptr(resolved) }.to_bytes().to_vec();
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

/// As best as possible, convert `p` into an absolute path, resolving
/// all symlinks along the way.
///
/// If `p` is a file url, it's converted to a path before this.
pub fn normalize_database_path(p: impl AsRef<Path>) -> Result<PathBuf> {
    let path = unurl_path(p);
    if let Ok(canonical) = canonicalize(&path) {
        return Ok(canonical);
    }
    // It probably doesn't exist yet. This is an error, although it seems to
    // work on some systems.
    //
    // We resolve this by trying to canonicalize the parent directory, and
    // appending the requested file name onto that. If we can't canonicalize
    // the parent, we return an error.
    //
    // Also, we return errors if the path ends in "..", if there is no
    // parent directory, etc.
    let file_name = path
        .file_name()
        .ok_or_else(|| Error::IllegalDatabasePath(path.clone()))?;

    let parent = path
        .parent()
        .ok_or_else(|| Error::IllegalDatabasePath(path.clone()))?;

    let mut canonical = canonicalize(parent)?;
    canonical.push(file_name);
    Ok(canonical)
}

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_unurl_path() {
        assert_eq!(
            unurl_path("file:///foo%20bar/baz").to_string_lossy(),
            "/foo bar/baz"
        );
        assert_eq!(unurl_path("/foo bar/baz").to_string_lossy(), "/foo bar/baz");
        assert_eq!(unurl_path("../baz").to_string_lossy(), "../baz");
    }

    #[test]
    fn test_normalize_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");
        std::fs::write(&path, b"").unwrap();

        assert_eq!(
            normalize_database_path(&path).unwrap(),
            canonicalize(&path).unwrap()
        );
    }

    #[test]
    fn test_normalize_nonexistent_leaf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");

        assert_eq!(
            normalize_database_path(&path).unwrap(),
            canonicalize(dir.path()).unwrap().join("places.sqlite")
        );
    }

    #[test]
    fn test_normalize_nonexistent_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("places.sqlite");

        assert!(matches!(
            normalize_database_path(&path),
            Err(Error::IoError(_))
        ));
    }

    #[test]
    fn test_normalize_malformed_path() {
        assert!(matches!(
            normalize_database_path(""),
            Err(Error::IllegalDatabasePath(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_file_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.sqlite");
        std::fs::write(&path, b"").unwrap();
        let url = Url::from_file_path(&path).unwrap();

        assert_eq!(
            normalize_database_path(url.as_str()).unwrap(),
            canonicalize(&path).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_resolves_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("places.sqlite");
        let link = dir.path().join("link.sqlite");
        std::fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert_eq!(
            normalize_database_path(&link).unwrap(),
            canonicalize(&target).unwrap()
        );
    }
}
