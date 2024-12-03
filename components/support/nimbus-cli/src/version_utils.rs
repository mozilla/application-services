// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::Result;

pub(crate) fn is_before(current_version: &Option<String>, upto_version: usize) -> bool {
    if current_version.is_none() {
        false
    } else {
        let current_version = current_version.as_deref().unwrap();
        is_between(0, current_version, upto_version).unwrap_or(false)
    }
}

fn is_between(min_version: usize, current_version: &str, max_version: usize) -> Result<bool> {
    let (major, _) = current_version
        .split_once('.')
        .unwrap_or((current_version, ""));
    let v = major.parse::<usize>()?;
    Ok(min_version <= v && v < max_version)
}

/// The following are dumb string manipulations to pad out a version number.
/// We might use a version library if we need much more functionality, but right now
/// it's isolated in a single file where it can be replaced as/when necessary.
///
/// pad_major_minor_patch will zero pad the minor and patch versions if they are not present.
/// 112.1.3 --> 112.1.3
/// 112.1   --> 112.1.0
/// 112     --> 112.0.0
pub(crate) fn pad_major_minor_patch(version: &str) -> String {
    match version_split(version) {
        (Some(_), Some(_), Some(_)) => version.to_owned(),
        (Some(major), Some(minor), None) => format!("{major}.{minor}.0"),
        (Some(major), None, None) => format!("{major}.0.0"),
        _ => format!("{version}.0.0"),
    }
}

/// pad_major_minor will zero pad the minor version if it is not present.
/// If the patch version is present, then it is left intact.
/// 112.1.3 --> 112.1.3
/// 112.1   --> 112.1
/// 112     --> 112.0
pub(crate) fn pad_major_minor(version: &str) -> String {
    match version_split(version) {
        (Some(_), Some(_), Some(_)) => version.to_owned(),
        (Some(major), Some(minor), None) => format!("{major}.{minor}"),
        (Some(major), None, None) => format!("{major}.0"),
        _ => format!("{version}.0"),
    }
}

/// pad_major will keep the string as it is.
/// If the minor and/or patch versions are present, then they are left intact.
/// 112.1.3 --> 112.1.3
/// 112.1   --> 112.1
/// 112     --> 112
pub(crate) fn pad_major(version: &str) -> String {
    version.to_owned()
}

fn version_split(version: &str) -> (Option<&str>, Option<&str>, Option<&str>) {
    let mut split = version.splitn(3, '.');
    (split.next(), split.next(), split.next())
}
