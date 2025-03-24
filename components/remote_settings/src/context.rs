/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json::{Map, Value};

/// Remote settings context object
///
/// This is used to filter the records returned. We always fetch all `records` from the
/// remote-settings storage. Some records could have a `filter_expression`.  If this is passed in
/// and the record has a `filter_expression`, then only returns where the expression is true will
/// be returned.
///
/// See https://remote-settings.readthedocs.io/en/latest/target-filters.html for details.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct RemoteSettingsContext {
    /// Name of the application (e.g. "Fenix" or "Firefox iOS")
    pub app_name: String,
    /// Application identifier, especially for mobile (e.g. "org.mozilla.fenix")
    pub app_id: String,
    /// The delivery channel of the application (e.g "nightly")
    pub channel: String,
    /// User visible version string (e.g. "1.0.3")
    pub app_version: Option<String>,
    /// Build identifier generated by the CI system (e.g. "1234/A")
    pub app_build: Option<String>,
    /// The architecture of the device, (e.g. "arm", "x86")
    pub architecture: Option<String>,
    /// The manufacturer of the device the application is running on
    pub device_manufacturer: Option<String>,
    /// The model of the device the application is running on
    pub device_model: Option<String>,
    /// The locale of the application during initialization (e.g. "es-ES")
    pub locale: Option<String>,
    /// The name of the operating system (e.g. "Android", "iOS", "Darwin", "Windows")
    pub os: Option<String>,
    /// The user-visible version of the operating system (e.g. "1.2.3")
    pub os_version: Option<String>,
    /// Android specific for targeting specific sdk versions
    pub android_sdk_version: Option<String>,
    /// Used for debug purposes as a way to match only developer builds, etc.
    pub debug_tag: Option<String>,
    /// The date the application installed the app
    pub installation_date: Option<i64>,
    /// The application's home directory
    pub home_directory: Option<String>,
    /// Contains attributes specific to the application, derived by the application
    pub custom_targeting_attributes: Option<Map<String, Value>>,
}

impl RemoteSettingsContext {
    /// Convert this into the `env` value for the remote settings JEXL filter
    ///
    /// https://remote-settings.readthedocs.io/en/latest/target-filters.html
    pub(crate) fn into_env(self) -> Value {
        let mut v = Map::new();
        v.insert("channel".to_string(), self.channel.into());
        if let Some(version) = self.app_version {
            v.insert("version".to_string(), version.into());
        }
        if let Some(locale) = self.locale {
            v.insert("locale".to_string(), locale.into());
        }
        let mut appinfo = Map::from_iter([("ID".to_string(), self.app_id.into())]);
        if let Some(os) = self.os {
            appinfo.insert("OS".to_string(), os.into());
        }
        v.insert("appinfo".to_string(), appinfo.into());
        if let Some(mut custom) = self.custom_targeting_attributes {
            if let Some(Value::String(form_factor)) = custom.remove("form_factor") {
                v.insert("form_factor".to_string(), form_factor.into());
            }
            if let Some(Value::String(country)) = custom.remove("country") {
                v.insert("country".to_string(), country.into());
            }
        }
        v.into()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    /// Test that the remote settings context is normalized to match
    /// https://remote-settings.readthedocs.io/en/latest/target-filters.html, regardless of what
    /// the fields are named in Rust.
    #[test]
    fn test_context_normalization() {
        let context = RemoteSettingsContext {
            app_name: "test-app".into(),
            app_id: "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
            channel: "beta".into(),
            app_version: Some("1.0.0".into()),
            app_build: Some("1234/A".into()),
            architecture: Some("8086".into()),
            device_manufacturer: Some("IBM".into()),
            device_model: Some("XT".into()),
            os: Some("MS-DOS".into()),
            os_version: Some("6.1".into()),
            locale: Some("en-US".into()),
            android_sdk_version: Some("11".into()),
            debug_tag: Some("debug-tag".into()),
            installation_date: Some(738936000),
            home_directory: Some("/home/appservices".into()),
            custom_targeting_attributes: Some(Map::from_iter([
                ("form_factor".into(), "tablet".into()),
                ("country".into(), "US".into()),
                ("other".into(), "other".into()),
            ])),
        };
        assert_eq!(
            context.into_env(),
            json!({
                // Official fields
                "version": "1.0.0",
                "channel": "beta",
                "locale": "en-US",
                "appinfo": {
                    "ID": "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}",
                    "OS": "MS-DOS",
                },
                // Unofficial fields that we need for Suggest geo-expansion.  These should be made
                // into official fields that both the Desktop and Rust client support.
                "form_factor": "tablet",
                "country": "US",
                // All other fields should be ignored.  We don't want to get users relying on
                // fields that are only supported by the Rust client.
            })
        );
    }
}
