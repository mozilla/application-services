// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use crate::{cli::Cli, LaunchableApp, NimbusApp};
use anyhow::{bail, Result};

impl TryFrom<&Cli> for LaunchableApp {
    type Error = anyhow::Error;
    fn try_from(value: &Cli) -> Result<Self> {
        let app = value.app.as_str();
        let channel = value.channel.as_str();
        let device_id = value.device_id.clone();

        let prefix = match app {
            "fenix" => Some("org.mozilla"),
            "focus_android" => Some("org.mozilla"),
            "firefox_ios" => Some("org.mozilla.ios"),
            "focus_ios" => Some("org.mozilla.ios"),
            _ => None,
        };

        let suffix = match app {
            "fenix" => Some(match channel {
                "developer" => "fenix.debug",
                "nightly" => "fenix",
                "beta" => "firefox_beta",
                "release" => "firefox",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, nightly, beta or release", app, channel)),
            }),
            "focus_android" => Some(match channel {
                "developer" => "focus.debug",
                "nightly" => "focus.nightly",
                "beta" => "focus.beta",
                "release" => "focus",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, nightly, beta or release", app, channel)),
            }),
            "firefox_ios" => Some(match channel {
                "developer" => "Fennec",
                "beta" => "FirefoxBeta",
                "release" => "Firefox",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, beta or release", app, channel)),
            }),
            "focus_ios" => Some(match channel {
                "developer" => "Focus",
                "beta" => "Focus",
                "release" => "Focus",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, beta or release", app, channel)),
            }),
            _ => None,
        };

        // Scheme for deeplinks.
        let scheme = match app {
            "fenix" => Some(match channel {
                // Firefox for Android defines per channel deeplink schemes in the app/build.gradle.
                // e.g. https://github.com/mozilla-mobile/firefox-android/blob/5d18e7ffe2f3e4505ea815d584d20e66ad10f515/fenix/app/build.gradle#L154
                "developer" => "fenix-dev",
                "nightly" => "fenix-nightly",
                "beta" => "fenix-beta",
                "release" => "fenix",
                _ => unreachable!(),
            }),
            "firefox_ios" => Some(match channel {
                // Firefox for iOS uses MOZ_PUBLIC_URL_SCHEME, which is always
                // [`firefox`](https://github.com/mozilla-mobile/firefox-ios/blob/f1acc8a2232a736e65e235b811372ddbf3e802f8/Client/Configuration/Common.xcconfig#L24)
                // and MOZ_INTERNAL_URL_SCHEME which is different per channel.
                // e.g. https://github.com/mozilla-mobile/firefox-ios/blob/f1acc8a2232a736e65e235b811372ddbf3e802f8/Client/Configuration/Firefox.xcconfig#L12
                // From inspection of the code, there are no different uses for the internal vs
                // public, so very useful for launching the specific app on a phone where you
                // have multiple versions installed.
                "developer" => "fennec",
                "beta" => "firefox-beta",
                "release" => "firefox-internal",
                _ => unreachable!(),
            }),
            // Focus for iOS has two, firefox-focus and firefox-klar
            // It's not clear if Focus's channels are configured for this
            "focus_ios" => Some("firefox-focus"),

            // Focus for Android provides no deeplinks.
            _ => None,
        }
        .map(str::to_string);

        Ok(match (app, prefix, suffix) {
            ("fenix", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: ".App".to_string(),
                device_id,
                scheme,
            },
            ("focus_android", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: "org.mozilla.focus.activity.MainActivity".to_string(),
                device_id,
                scheme,
            },
            ("firefox_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
                scheme,
            },
            ("focus_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
                scheme,
            },
            _ => unimplemented!(),
        })
    }
}

impl NimbusApp {
    pub(crate) fn ref_from_version(&self, version: &Option<String>, ref_: &String) -> String {
        if version.is_none() {
            return ref_.to_string();
        }
        let version = version.as_ref().unwrap();
        match self.app_name.as_str() {
            // Fenix and Focus are both in the same repo, so should have the
            // same branching structure.
            "fenix" | "focus_android" => format!("releases_v{version}"),
            "firefox_ios" => format!("release/v{version}"),
            "focus_ios" => format!("releases_v{version}"),

            _ => unreachable!("{} is not defined", self.app_name),
        }
    }

    pub(crate) fn github_repo<'a>(&self) -> &'a str {
        match self.app_name.as_str() {
            // Fenix and Focus are both in the same repo
            "fenix" | "focus_android" => "mozilla-mobile/firefox-android",
            "firefox_ios" => "mozilla-mobile/firefox-ios",
            "focus_ios" => "mozilla-mobile/focus-ios",
            _ => unreachable!("{} is not defined", self.app_name),
        }
    }

    pub(crate) fn manifest_location<'a>(&self) -> &'a str {
        match self.app_name.as_str() {
            "fenix" => "fenix/app/nimbus.fml.yaml",
            "focus_android" => "focus-android/app/nimbus.fml.yaml",
            "firefox_ios" => "nimbus.fml.yaml",
            "focus_ios" => "nimbus.fml.yaml",
            _ => unreachable!("{} is not defined", self.app_name),
        }
    }
}

pub(crate) fn release_server() -> String {
    std::env::var("NIMBUS_URL")
        .unwrap_or_else(|_| "https://firefox.settings.services.mozilla.com".to_string())
}

pub(crate) fn stage_server() -> String {
    std::env::var("NIMBUS_URL_STAGE")
        .unwrap_or_else(|_| "https://firefox.settings.services.allizom.org".to_string())
}

pub(crate) fn manifest_cache_dir() -> PathBuf {
    match std::env::var("NIMBUS_MANIFEST_CACHE") {
        Ok(s) => {
            let cwd = std::env::current_dir().expect("Current Working Directory is not set");
            cwd.join(s)
        }
        _ => std::env::temp_dir(),
    }
}
