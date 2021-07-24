/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//  N.B. if you're making a documentation change here, you might also want to make it in:
//
//    * The API docs in ../ios/Logins/LoginRecord.swift
//    * The API docs in ../android/src/main/java/mozilla/appservices/logins/ServerPassword.kt
//    * The android-components docs at
//      https://github.com/mozilla-mobile/android-components/tree/master/components/service/sync-logins
//
//  We'll figure out a more scalable approach to maintaining all those docs at some point...

//!
//! # Login Struct
//!
//! The core datatype managed by this component is a "Login" struct. Very closely related is the
//! "sync payload", defined in sync/payload.rs, which handles all aspects of the JSON serialization
//! and deserialization to and from these structs - but all validation etc. is done by this core
//! Login struct. It contains the following fields:
//!
//! - `id`:  A unique string identifier for this record.
//!
//!   Consumers may assume that `id` contains only "safe" ASCII characters but should otherwise
//!   treat this it as an opaque identifier. It should be left blank when adding a new record,
//!   in which case a new id will be automatically generated.
//!
//! - `origin`:  The origin at which this login can be used, as a string.
//!
//!   The login should only be used on sites that match this origin (for whatever definition
//!   of "matches" makes sense at the application level, e.g. eTLD+1 matching).
//!   This field is required, must be a valid origin in punycode format, and must not be
//!   set to the empty string.
//!
//!   Examples of valid `origin` values include:
//!   - "https://site.com"
//!   - "http://site.com:1234"
//!   - "ftp://ftp.site.com"
//!   - "moz-proxy://127.0.0.1:8888"
//!   - "chrome://MyLegacyExtension"
//!   - "file://"
//!   - "https://[::1]"
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - truncating full URLs to just their origin component, if it is not an opaque origin
//!   - converting values with non-ascii characters into punycode
//!
//!   **XXX TODO:**
//!   - Add a field with the original unicode versions of the URLs instead of punycode?
//!
//! - `enc_fields`: The `username` and `password` for the site, stored as a encrypted JSON
//!    representation of an `EncryptedFields`.
//!
//!   This field is required and usually encryted.  There are two different value types:
//!       - Plantext empty string: Used for deleted records
//!       - Encrypted value: The credentials associated with the login.
//!
//! - `http_realm`:  The challenge string for HTTP Basic authentication, if any.
//!
//!   If present, the login should only be used in response to a HTTP Basic Auth
//!   challenge that specifies a matching realm. For legacy reasons this string may not
//!   contain null bytes, carriage returns or newlines.
//!
//!   If this field is set to the empty string, this indicates a wildcard match on realm.
//!
//!   This field must not be present if `form_action_origin` is set, since they indicate different types
//!   of login (HTTP-Auth based versus form-based). Exactly one of `http_realm` and `form_action_origin`
//!   must be present.
//!
//! - `form_action_origin`:  The target origin of forms in which this login can be used, if any, as a string.
//!
//!   If present, the login should only be used in forms whose target submission URL matches this origin.
//!   This field must be a valid origin or one of the following special cases:
//!   - An empty string, which is a wildcard match for any origin.
//!   - The single character ".", which is equivalent to the empty string
//!   - The string "javascript:", which matches any form with javascript target URL.
//!
//!   This field must not be present if `http_realm` is set, since they indicate different types of login
//!   (HTTP-Auth based versus form-based). Exactly one of `http_realm` and `form_action_origin` must be present.
//!
//!   If invalid data is received in this field (either from the application, or via sync) then the
//!   logins store will attempt to coerce it into valid data by:
//!   - truncating full URLs to just their origin component
//!   - converting origins with non-ascii characters into punycode
//!   - replacing invalid values with null if a valid 'http_realm' field is present
//!
//! - `username_field`:  The name of the form field into which the 'username' should be filled, if any.
//!
//!   This value is stored if provided by the application, but does not imply any restrictions on
//!   how the login may be used in practice. For legacy reasons this string may not contain null
//!   bytes, carriage returns or newlines. This field must be empty unless `form_action_origin` is set.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - setting to the empty string if 'form_action_origin' is not present
//!
//! - `password_field`:  The name of the form field into which the 'password' should be filled, if any.
//!
//!   This value is stored if provided by the application, but does not imply any restrictions on
//!   how the login may be used in practice. For legacy reasons this string may not contain null
//!   bytes, carriage returns or newlines. This field must be empty unless `form_action_origin` is set.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - setting to the empty string if 'form_action_origin' is not present
//!
//! - `timesUsed`:  A lower bound on the number of times the password from this record has been used, as an integer.
//!
//!   Applications should use the `touch()` method of the logins store to indicate when a password
//!   has been used, and should ensure that they only count uses of the actual `password` field
//!   (so for example, copying the `password` field to the clipboard should count as a "use", but
//!   copying just the `username` field should not).
//!
//!   This number may not record uses that occurred on other devices, since some legacy
//!   sync clients do not record this information. It may be zero for records obtained
//!   via sync that have never been used locally.
//!
//!   When merging duplicate records, the two usage counts are summed.
//!
//!   This field is managed internally by the logins store by default and does not need to
//!   be set explicitly, although any application-provided value will be preserved when creating
//!   a new record.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - replacing missing or negative values with 0
//!
//!   **XXX TODO:**
//!   - test that we prevent this counter from moving backwards.
//!   - test fixups of missing or negative values
//!   - test that we correctly merge dupes
//!
//! - `time_created`: An upper bound on the time of creation of this login, in integer milliseconds from the unix epoch.
//!
//!   This is an upper bound because some legacy sync clients do not record this information.
//!
//!   Note that this field is typically a timestamp taken from the local machine clock, so it
//!   may be wildly inaccurate if the client does not have an accurate clock.
//!
//!   This field is managed internally by the logins store by default and does not need to
//!   be set explicitly, although any application-provided value will be preserved when creating
//!   a new record.
//!
//!   When merging duplicate records, the smallest non-zero value is taken.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - replacing missing or negative values with the current time
//!
//!   **XXX TODO:**
//!   - test that we prevent this timestamp from moving backwards.
//!   - test fixups of missing or negative values
//!   - test that we correctly merge dupes
//!
//! - `time_last_used`: A lower bound on the time of last use of this login, in integer milliseconds from the unix epoch.
//!
//!   This is a lower bound because some legacy sync clients do not record this information;
//!   in that case newer clients set `timeLastUsed` when they use the record for the first time.
//!
//!   Note that this field is typically a timestamp taken from the local machine clock, so it
//!   may be wildly inaccurate if the client does not have an accurate clock.
//!
//!   This field is managed internally by the logins store by default and does not need to
//!   be set explicitly, although any application-provided value will be preserved when creating
//!   a new record.
//!
//!   When merging duplicate records, the largest non-zero value is taken.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - removing negative values
//!
//!   **XXX TODO:**
//!   - test that we prevent this timestamp from moving backwards.
//!   - test fixups of missing or negative values
//!   - test that we correctly merge dupes
//!
//! - `time_password_changed`: A lower bound on the time that the `password` field was last changed, in integer
//!                          milliseconds from the unix epoch.
//!
//!   Changes to other fields (such as `username`) are not reflected in this timestamp.
//!   This is a lower bound because some legacy sync clients do not record this information;
//!   in that case newer clients set `time_password_changed` when they change the `password` field.
//!
//!   Note that this field is typically a timestamp taken from the local machine clock, so it
//!   may be wildly inaccurate if the client does not have an accurate clock.
//!
//!   This field is managed internally by the logins store by default and does not need to
//!   be set explicitly, although any application-provided value will be preserved when creating
//!   a new record.
//!
//!   When merging duplicate records, the largest non-zero value is taken.
//!
//!   If invalid data is received in this field (either from the application, or via sync)
//!   then the logins store will attempt to coerce it into valid data by:
//!   - removing negative values
//!
//!   **XXX TODO:**
//!   - test that we prevent this timestamp from moving backwards.
//!   - test that we don't set this for changes to other fields.
//!   - test that we correctly merge dupes
//!
//! In order to deal with data from legacy clients in a robust way, it is necessary to be able to build
//! and manipulate `Login` structs that contain invalid data.  The following methods can be used by callers
//! to ensure that they're only working with valid records:
//!
//! - `Login::check_valid()`:    Checks valdity of a login record, returning `()` if it is valid
//!                              or an error if it is not.
//!
//! - `Login::fixup()`:   Returns either the existing login if it is valid, a clone with invalid fields
//!                       fixed up if it was safe to do so, or an error if the login is irreparably invalid.
//!
//!
//! # UpdatableLogin Struct
//! This struct is a subset of the Login struct, consisting only of the fields which an 'update()'
//! call can change. It also has the decrypted version of the credentials - they will be encrypted
//! before being added to the database. Thus, almost all uses of an UpdatableLogin struct will
//! also require the encryption key.

use crate::{encryption::EncryptorDecryptor, error::*};
use rusqlite::Row;
use serde_derive::*;
use sync_guid::Guid;
use url::Url;

// This is used to add or update items. It has no meta-data and has the "encrypted" fields as plain
// text. To use this, you almost certainly also need an `EncryptorDecryptor` too.
#[derive(Debug, Clone, Hash, PartialEq, Default)]
pub struct UpdatableLogin {
    pub origin: String,
    pub form_action_origin: Option<String>,
    pub http_realm: Option<String>,
    pub username_field: String,
    pub password_field: String,
    pub enc_fields: EncryptedFields,
}

impl UpdatableLogin {
    /// Checks whether the Login is valid, without attempting to fix any fields.
    /// Returns an error if invalid data is found, even if it could have been fixed.
    pub fn check_valid(&self) -> Result<()> {
        self.validate_and_fixup(false)?;
        Ok(())
    }

    /// Return either the existing login, a fixed-up verion, or an error.
    /// This consumes `self` to make it easy for callers to unconditionally
    /// replace a Login with an owned fixed-up version, preventing them from
    /// using one that is invalid.
    pub fn fixup(self) -> Result<Self> {
        match self.maybe_fixup()? {
            None => Ok(self),
            Some(login) => Ok(login),
        }
    }

    /// Like `fixup()` above, but takes `self` by reference and returns
    /// an Option for the fixed-up version, allowing the caller to make
    /// more choices about what to do next.
    pub fn maybe_fixup(&self) -> Result<Option<Self>> {
        self.validate_and_fixup(true)
    }

    /// Internal helper for validation and fixups of an "origin" stored as
    /// a string.
    fn validate_and_fixup_origin(origin: &str) -> Result<Option<String>> {
        // Check we can parse the origin, then use the normalized version of it.
        match Url::parse(&origin) {
            Ok(mut u) => {
                // Presumably this is a faster path than always setting?
                if u.path() != "/"
                    || u.fragment().is_some()
                    || u.query().is_some()
                    || u.username() != "/"
                    || u.password().is_some()
                {
                    // Not identical - we only want the origin part, so kill
                    // any other parts which may exist.
                    // But first special case `file://` URLs which always
                    // resolve to `file://`
                    if u.scheme() == "file" {
                        return Ok(if origin == "file://" {
                            None
                        } else {
                            Some("file://".into())
                        });
                    }
                    u.set_path("");
                    u.set_fragment(None);
                    u.set_query(None);
                    let _ = u.set_username("");
                    let _ = u.set_password(None);
                    let mut href = String::from(u);
                    // We always store without the trailing "/" which Urls have.
                    if href.ends_with('/') {
                        href.pop().expect("url must have a length");
                    }
                    if origin != href {
                        // Needs to be fixed up.
                        return Ok(Some(href));
                    }
                }
                Ok(None)
            }
            Err(_) => {
                // We can't fixup completely invalid records, so always throw.
                throw!(InvalidLogin::IllegalFieldValue {
                    field_info: "Origin is Malformed".into()
                });
            }
        }
    }

    /// Internal helper for doing validation and fixups.
    fn validate_and_fixup(&self, fixup: bool) -> Result<Option<Self>> {
        // XXX TODO: we've definitely got more validation and fixups to add here!

        let mut maybe_fixed = None;

        /// A little helper to magic a Some(self.clone()) into existence when needed.
        macro_rules! get_fixed_or_throw {
            ($err:expr) => {
                // This is a block expression returning a local variable,
                // entirely so we can give it an explicit type declaration.
                {
                    if !fixup {
                        throw!($err)
                    }
                    log::warn!("Fixing login record {:?}", $err);
                    let fixed: Result<&mut Self> =
                        Ok(maybe_fixed.get_or_insert_with(|| self.clone()));
                    fixed
                }
            };
        }

        if self.origin.is_empty() {
            throw!(InvalidLogin::EmptyOrigin);
        }

        if self.enc_fields.password.is_empty() {
            throw!(InvalidLogin::EmptyPassword);
        }

        if self.form_action_origin.is_some() && self.http_realm.is_some() {
            get_fixed_or_throw!(InvalidLogin::BothTargets)?.http_realm = None;
        }

        if self.form_action_origin.is_none() && self.http_realm.is_none() {
            throw!(InvalidLogin::NoTarget);
        }

        let form_action_origin = self.form_action_origin.clone().unwrap_or_default();
        let http_realm = maybe_fixed
            .as_ref()
            .unwrap_or(self)
            .http_realm
            .clone()
            .unwrap_or_default();

        let field_data = [
            ("form_action_origin", &form_action_origin),
            ("http_realm", &http_realm),
            ("origin", &self.origin),
            ("username_field", &self.username_field),
            ("password_field", &self.password_field),
            ("username", &self.enc_fields.username),
            ("password", &self.enc_fields.password),
        ];

        for (field_name, field_value) in &field_data {
            // Nuls are invalid.
            if field_value.contains('\0') {
                throw!(InvalidLogin::IllegalFieldValue {
                    field_info: format!("`{}` contains Nul", field_name)
                });
            }

            // Newlines are invalid in Desktop with the exception of the username
            // and password fields.
            if field_name != &"username"
                && field_name != &"password"
                && (field_value.contains('\n') || field_value.contains('\r'))
            {
                throw!(InvalidLogin::IllegalFieldValue {
                    field_info: format!("`{}` contains newline", field_name)
                });
            }
        }

        // Desktop doesn't like fields with the below patterns
        if self.username_field == "." {
            throw!(InvalidLogin::IllegalFieldValue {
                field_info: "`username_field` is a period".into()
            });
        }

        // Check we can parse the origin, then use the normalized version of it.
        if let Some(fixed) = Self::validate_and_fixup_origin(&self.origin)? {
            get_fixed_or_throw!(InvalidLogin::IllegalFieldValue {
                field_info: "Origin is not normalized".into()
            })?
            .origin = fixed;
        }

        match &maybe_fixed.as_ref().unwrap_or(self).form_action_origin {
            None => {
                if !self.username_field.is_empty() {
                    get_fixed_or_throw!(InvalidLogin::IllegalFieldValue {
                        field_info: "username_field must be empty when form_action_origin is null"
                            .into()
                    })?
                    .username_field
                    .clear();
                }
                if !self.password_field.is_empty() {
                    get_fixed_or_throw!(InvalidLogin::IllegalFieldValue {
                        field_info: "password_field must be empty when form_action_origin is null"
                            .into()
                    })?
                    .password_field
                    .clear();
                }
            }
            Some(href) => {
                // "", ".", and "javascript:" are special cases documented at the top of this file.
                if href == "." {
                    // A bit of a special case - if we are being asked to fixup, we replace
                    // "." with an empty string - but if not fixing up we don't complain.
                    if fixup {
                        maybe_fixed
                            .get_or_insert_with(|| self.clone())
                            .form_action_origin = Some("".into());
                    }
                } else if !href.is_empty() && href != "javascript:" {
                    if let Some(fixed) = Self::validate_and_fixup_origin(&href)? {
                        get_fixed_or_throw!(InvalidLogin::IllegalFieldValue {
                            field_info: "form_action_origin is not normalized".into()
                        })?
                        .form_action_origin = Some(fixed);
                    }
                }
            }
        }

        Ok(maybe_fixed)
    }
}

/// This is what you get when reading from the store. It has all metadata but
/// the encrypted fields are encrypted.
#[derive(Debug, Clone, Hash, PartialEq, Default)]
pub struct Login {
    pub id: String,
    pub origin: String,
    pub form_action_origin: Option<String>,
    pub http_realm: Option<String>,
    pub username_field: String,
    pub password_field: String,
    pub time_created: i64,
    pub time_password_changed: i64,
    pub time_last_used: i64,
    pub times_used: i64,
    pub enc_fields: String,
}

impl Login {
    #[inline]
    pub fn guid(&self) -> Guid {
        Guid::from_string(self.id.clone())
    }
    // TODO: Remove this: https://github.com/mozilla/application-services/issues/4185
    #[inline]
    pub fn guid_str(&self) -> &str {
        &self.id
    }

    pub fn decrypt_fields(&self, encdec: &EncryptorDecryptor) -> Result<EncryptedFields> {
        encdec.decrypt_struct(&self.enc_fields)
    }

    pub(crate) fn from_row(row: &Row<'_>) -> Result<Login> {
        let login = Login {
            id: row.get("guid")?,
            enc_fields: row.get("encFields")?,

            origin: row.get("origin")?,
            http_realm: row.get("httpRealm")?,

            form_action_origin: row.get("formActionOrigin")?,

            username_field: string_or_default(row, "usernameField")?,
            password_field: string_or_default(row, "passwordField")?,

            time_created: row.get("timeCreated")?,
            // Might be null
            time_last_used: row
                .get::<_, Option<i64>>("timeLastUsed")?
                .unwrap_or_default(),

            time_password_changed: row.get("timePasswordChanged")?,
            times_used: row.get("timesUsed")?,
        };
        // XXX - we used to perform a fixup here, but that seems heavy-handed
        // and difficult - we now only do that on add/insert when we have the
        // encryption key.
        Ok(login)
    }

    // This is useful for tests, and rust consumers, like the sync-pass example.
    // It helps with a read/update cycle. Consumes the item.
    pub fn into_updatable(self, encdec: &EncryptorDecryptor) -> Result<UpdatableLogin> {
        Ok(UpdatableLogin {
            enc_fields: self.decrypt_fields(encdec)?,
            origin: self.origin,
            form_action_origin: self.form_action_origin,
            http_realm: self.http_realm,
            username_field: self.username_field,
            password_field: self.password_field,
        })
    }
}

/// The encrypted fields.
#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
pub struct EncryptedFields {
    // prior to per-field encryption the `username` was allowed to be null.
    // We could make it `Option<String>` but that doesn't seem valuable, as
    // an empty string and None mean the same thing anyway.
    // XXX - although this might end up being important in the sync payload (as
    // in, the item missing entirely from the payload?)
    // We should check desktop's semantics.
    pub username: String,
    pub password: String,
}

impl EncryptedFields {
    pub fn encrypt(&self, encdec: &EncryptorDecryptor) -> Result<String> {
        encdec.encrypt_struct(&self)
    }
}

fn string_or_default(row: &Row<'_>, col: &str) -> Result<String> {
    Ok(row.get::<_, Option<String>>(col)?.unwrap_or_default())
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::encryption::test_utils::encrypt_struct;

    // Factory function to make a new login
    //
    // It uses the guid to create a unique origin/form_action_origin
    pub fn login(id: &str, password: &str) -> Login {
        let enc_fields = EncryptedFields {
            username: "user".to_string(),
            password: password.to_string(),
        };
        Login {
            id: id.to_string(),
            form_action_origin: Some(format!("https://{}.example.com", id)),
            origin: format!("https://{}.example.com", id),
            enc_fields: encrypt_struct(&enc_fields),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_fixups() -> Result<()> {
        // Start with URLs which are all valid and already normalized.
        for input in &[
            // The list of valid origins documented at the top of this file.
            "https://site.com",
            "http://site.com:1234",
            "ftp://ftp.site.com",
            "moz-proxy://127.0.0.1:8888",
            "chrome://MyLegacyExtension",
            "file://",
            "https://[::1]",
        ] {
            assert_eq!(UpdatableLogin::validate_and_fixup_origin(input)?, None);
        }

        // And URLs which get normalized.
        for (input, output) in &[
            ("https://site.com/", "https://site.com"),
            ("http://site.com:1234/", "http://site.com:1234"),
            ("http://example.com/foo?query=wtf#bar", "http://example.com"),
            ("http://example.com/foo#bar", "http://example.com"),
            (
                "http://username:password@example.com/",
                "http://example.com",
            ),
            ("http://😍.com/", "http://xn--r28h.com"),
            ("https://[0:0:0:0:0:0:0:1]", "https://[::1]"),
            // All `file://` URLs normalize to exactly `file://`. See #2384 for
            // why we might consider changing that later.
            ("file:///", "file://"),
            ("file://foo/bar", "file://"),
            ("file://foo/bar/", "file://"),
            ("moz-proxy://127.0.0.1:8888/", "moz-proxy://127.0.0.1:8888"),
            (
                "moz-proxy://127.0.0.1:8888/foo",
                "moz-proxy://127.0.0.1:8888",
            ),
            ("chrome://MyLegacyExtension/", "chrome://MyLegacyExtension"),
            (
                "chrome://MyLegacyExtension/foo",
                "chrome://MyLegacyExtension",
            ),
        ] {
            assert_eq!(
                UpdatableLogin::validate_and_fixup_origin(input)?,
                Some((*output).into())
            );
        }
        Ok(())
    }

    #[test]
    fn test_check_valid() {
        #[derive(Debug, Clone)]
        struct TestCase {
            login: UpdatableLogin,
            should_err: bool,
            expected_err: &'static str,
        }

        let valid_login = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_empty_origin = UpdatableLogin {
            origin: "".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_empty_password = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "".into(),
            },
            ..Default::default()
        };

        let login_with_form_submit_and_http_realm = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            form_action_origin: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_without_form_submit_or_http_realm = UpdatableLogin {
            origin: "https://www.example.com".into(),
            enc_fields: EncryptedFields {
                username: "".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_null_http_realm = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.\0com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_null_username = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "\0".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_null_password = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "username".into(),
                password: "test\0".into(),
            },
            ..Default::default()
        };

        let login_with_newline_origin = UpdatableLogin {
            origin: "\rhttps://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_newline_username_field = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            username_field: "\n".into(),
            ..Default::default()
        };

        let login_with_newline_realm = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("foo\nbar".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_newline_password = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test\n".into(),
            },
            ..Default::default()
        };

        let login_with_period_username_field = UpdatableLogin {
            origin: "https://www.example.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            username_field: ".".into(),
            ..Default::default()
        };

        let login_with_period_form_action_origin = UpdatableLogin {
            form_action_origin: Some(".".into()),
            origin: "https://www.example.com".into(),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_javascript_form_action_origin = UpdatableLogin {
            form_action_origin: Some("javascript:".into()),
            origin: "https://www.example.com".into(),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_malformed_origin_parens = UpdatableLogin {
            origin: " (".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_host_unicode = UpdatableLogin {
            origin: "http://💖.com".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_origin_trailing_slash = UpdatableLogin {
            origin: "https://www.example.com/".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_origin_expanded_ipv6 = UpdatableLogin {
            origin: "https://[0:0:0:0:0:0:1:1]".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_unknown_protocol = UpdatableLogin {
            origin: "moz-proxy://127.0.0.1:8888".into(),
            http_realm: Some("https://www.example.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let test_cases = [
            TestCase {
                login: valid_login,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                login: login_with_empty_origin,
                should_err: true,
                expected_err: "Invalid login: Origin is empty",
            },
            TestCase {
                login: login_with_empty_password,
                should_err: true,
                expected_err: "Invalid login: Password is empty",
            },
            TestCase {
                login: login_with_form_submit_and_http_realm,
                should_err: true,
                expected_err: "Invalid login: Both `formActionOrigin` and `httpRealm` are present",
            },
            TestCase {
                login: login_without_form_submit_or_http_realm,
                should_err: true,
                expected_err:
                    "Invalid login: Neither `formActionOrigin` or `httpRealm` are present",
            },
            TestCase {
                login: login_with_null_http_realm,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: `http_realm` contains Nul",
            },
            TestCase {
                login: login_with_null_username,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: `username` contains Nul",
            },
            TestCase {
                login: login_with_null_password,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: `password` contains Nul",
            },
            TestCase {
                login: login_with_newline_origin,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: `origin` contains newline",
            },
            TestCase {
                login: login_with_newline_realm,
                should_err: true,
                expected_err:
                    "Invalid login: Login has illegal field: `http_realm` contains newline",
            },
            TestCase {
                login: login_with_newline_username_field,
                should_err: true,
                expected_err:
                    "Invalid login: Login has illegal field: `username_field` contains newline",
            },
            TestCase {
                login: login_with_newline_password,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                login: login_with_period_username_field,
                should_err: true,
                expected_err:
                    "Invalid login: Login has illegal field: `username_field` is a period",
            },
            TestCase {
                login: login_with_period_form_action_origin,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                login: login_with_javascript_form_action_origin,
                should_err: false,
                expected_err: "",
            },
            TestCase {
                login: login_with_malformed_origin_parens,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: Origin is Malformed",
            },
            TestCase {
                login: login_with_host_unicode,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: Origin is not normalized",
            },
            TestCase {
                login: login_with_origin_trailing_slash,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: Origin is not normalized",
            },
            TestCase {
                login: login_with_origin_expanded_ipv6,
                should_err: true,
                expected_err: "Invalid login: Login has illegal field: Origin is not normalized",
            },
            TestCase {
                login: login_with_unknown_protocol,
                should_err: false,
                expected_err: "",
            },
        ];

        for tc in &test_cases {
            let actual = tc.login.check_valid();

            if tc.should_err {
                assert!(actual.is_err(), "{:#?}", tc);
                assert_eq!(
                    tc.expected_err,
                    actual.unwrap_err().to_string(),
                    "{:#?}",
                    tc,
                );
            } else {
                assert!(actual.is_ok(), "{:#?}", tc);
                assert!(
                    tc.login.clone().fixup().is_ok(),
                    "Fixup failed after check_valid passed: {:#?}",
                    &tc,
                );
            }
        }
    }

    #[test]
    fn test_fixup() {
        #[derive(Debug, Default)]
        struct TestCase {
            login: UpdatableLogin,
            fixedup_host: Option<&'static str>,
            fixedup_form_action_origin: Option<String>,
        }

        // Note that most URL fixups are tested above, but we have one or 2 here.
        let login_with_full_url = UpdatableLogin {
            origin: "http://example.com/foo?query=wtf#bar".into(),
            form_action_origin: Some("http://example.com/foo?query=wtf#bar".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_host_unicode = UpdatableLogin {
            origin: "http://😍.com".into(),
            form_action_origin: Some("http://😍.com".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_period_fsu = UpdatableLogin {
            origin: "https://example.com".into(),
            form_action_origin: Some(".".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };
        let login_with_empty_fsu = UpdatableLogin {
            origin: "https://example.com".into(),
            form_action_origin: Some("".into()),
            enc_fields: EncryptedFields {
                username: "test".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let login_with_form_submit_and_http_realm = UpdatableLogin {
            origin: "https://www.example.com".into(),
            form_action_origin: Some("https://www.example.com".into()),
            // If both http_realm and form_action_origin are specified, we drop
            // the former when fixing up. So for this test we must have an
            // invalid value in http_realm to ensure we don't validate a value
            // we end up dropping.
            http_realm: Some("\n".into()),
            enc_fields: EncryptedFields {
                username: "".into(),
                password: "test".into(),
            },
            ..Default::default()
        };

        let test_cases = [
            TestCase {
                login: login_with_full_url,
                fixedup_host: "http://example.com".into(),
                fixedup_form_action_origin: Some("http://example.com".into()),
            },
            TestCase {
                login: login_with_host_unicode,
                fixedup_host: "http://xn--r28h.com".into(),
                fixedup_form_action_origin: Some("http://xn--r28h.com".into()),
            },
            TestCase {
                login: login_with_period_fsu,
                fixedup_form_action_origin: Some("".into()),
                ..TestCase::default()
            },
            TestCase {
                login: login_with_form_submit_and_http_realm,
                fixedup_form_action_origin: Some("https://www.example.com".into()),
                ..TestCase::default()
            },
            TestCase {
                login: login_with_empty_fsu,
                // Should still be empty.
                fixedup_form_action_origin: Some("".into()),
                ..TestCase::default()
            },
        ];

        for tc in &test_cases {
            let login = tc.login.clone().fixup().expect("should work");
            if let Some(expected) = tc.fixedup_host {
                assert_eq!(login.origin, expected, "origin not fixed in {:#?}", tc);
            }
            assert_eq!(
                login.form_action_origin, tc.fixedup_form_action_origin,
                "form_action_origin not fixed in {:#?}",
                tc,
            );
            login.check_valid().unwrap_or_else(|e| {
                panic!("Fixup produces invalid record: {:#?}", (e, &tc, &login));
            });
            assert_eq!(
                login.clone().fixup().unwrap(),
                login,
                "fixup did not reach fixed point for testcase: {:#?}",
                tc,
            );
        }
    }
}
