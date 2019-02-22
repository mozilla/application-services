/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::LoginDb;
use crate::login::Login;
use crate::util;
use url::{Origin, Url};
use crate::error::Result;

pub fn has_bad_character(field: &str) -> bool {
    memchr::memchr3(b'\r', b'\n', b'\0', field.as_bytes()).is_some()
}

/// Fixes `l` in the following ways:
///
/// 1. If possible, ensure `hostname` is a sane origin.
///     - Failing it being a sane origin, we tries to ensure it's a parsable URL
///     - Failing that, leave it alone.
///
/// 2. Reconcile inconsistent or corrupted `formSubmitURL` and `httpRealm` values.
///     - If neither are present, assume `form_submit_url` is actually ""
///       and some client messed it up.
///
///     - If both are present, resolve in favor of `httpRealm` unless it's an
///       empty string, or contains characters that cause desktop to barf
///       (`\n`, `\r`, `\0`).
///
///     - If only `formSubmitURL` is present, or both are present but we don't want
///       the httpRealm value, use `formSubmitURL`, possibly fixing it up
///         - If it's an empty string, we use the empty string.
///         - Otherwise, attempt to use it's origin, if we can get one.
///         - If we can't, but we can get a parsable URL, use that.
///         - Otherwise, use the (fixed up version of the) hostname.
///
///     - If only `httpRealm` is present and it only contains valid characters,
///       take it.
///
///     - If only `httpRealm` is present and it contains `\r`, `\n`, or `\0`,
///       replace them with spaces. (This is not likely to work for logging in,
///       but seems better than discarding a password the user may not remember).
///
/// 3. If `username_field` or `password_field` contain characters that would break
///    desktop, then set the fields to "". Those fields are optional and
///    semi-deprecated anyway.
///
/// 4. Remove any `\0` that happen to exist in `username` or `password`. This is
///    slightly dubious, but it's unclear to me what the better option is.
///    Deletion seems bad, and it's likely to cause us problems in the FFI (at
///    least, before we get around to using protobufs)
fn fix_login(l: &Login) -> Login {
    // Wasteful but whatever.
    let mut fixed = l.clone();
    let hostname = if let Some(fixed) = try_fixup_origin_string(&l.hostname, true, false) {
        fixed
    } else {
        // Hostname is extremely important, but I'm just going to keep
        // the original if we can't fix it.
        l.hostname.clone()
    };

    if hostname != l.hostname {
        log::trace!("  Fixing {} hostname", l.id);
        fixed.hostname = hostname.into();
    }

    let (next_url, next_realm) = match (&l.form_submit_url, &l.http_realm) {
        (None, None) => {
            // If they're both missing, assume the empty form url is supposed to
            // be the empty string and some client (maybe us in a previous version!)
            // confused things.
            (Some("".into()), None)
        }

        (Some(_), Some(realm)) if !realm.is_empty() && !has_bad_character(realm) => {
            // If the realm exists and is valid, go with that over the form
            // url unless it's empty.
            (None, Some(realm.clone()))
        }

        // If the URL exists, and the realm doesn't or we can't use it,
        // then use the url, possibly fixing it up and turning it into an
        // origin as needed. In the case of a url that doesn't parse as
        // such, either use the empty string (if it was empty/entirely
        // whitespace), or the hostname.
        (Some(url), _) => {
            // No realm, or empty/corrupt realm.
            let res_url = if let Some(s) = try_fixup_origin_string(url, true, true) {
                s
            } else {
                // Can't borrow from fixed.hostname since we need
                // to assign to it later
                fixed.hostname.clone()
            };
            (Some(res_url), None)
        }

        // The 'only realm' case is straightforward, with the unfortunate
        // caveat that if the realm is not valid, we try to replace the
        // illegal characters with spaces. This probably won't actually work,
        // seems better than throwing the record away and possibly losing
        // a password that the user no longer remembers.
        //
        // In practice this should never happen, as 'invalid' means it has
        // characters which aren't even allowed in HTTP headers to begin with.
        (None, Some(realm)) => {
            if has_bad_character(realm) {
                log::trace!("  Fixup {}: Invalid realm", l.id);
                let realm = realm.replace(|c| c == '\r' || c == '\n' || c == '\0', " ");
                (None, Some(realm))
            } else {
                (None, Some(realm.clone()))
            }
        }
    };

    if next_url != l.form_submit_url {
        log::trace!("  Fixup {}: Changed form_submit_url", l.id);
        fixed.form_submit_url = next_url;
    }
    if next_realm != l.http_realm {
        // already logged about this.
        fixed.http_realm = next_realm.into();
    }

    // username_field and password_field are pseudo-deprecated (as far as I
    // understand it), so if they're causing problems, then clear them.
    if has_bad_character(&l.username_field) || l.username_field == "." {
        log::trace!("  Fixup {}: Invalid username_field", l.id);
        fixed.username_field.clear();
    }
    if has_bad_character(&l.password_field) {
        log::trace!("  Fixup {}: Invalid password_field", l.id);
        fixed.password_field.clear();
    }

    // This is wrong, but should be so rare that doesn't matter.
    // Remove '\0' from the username and password, if present.
    fixed.password = l.password.replace('\0', "");
    fixed.username = l.username.replace('\0', "");

    fixed
}

pub fn maybe_fixup_logins(db: &LoginDb) -> Result<()> {
    // If we've ever done the fixup, don't bother doing it again. Eventually
    // we might want to change that, but for now it should just be a 'run once'
    // thing
    if db.get_meta::<i64>(crate::schema::LAST_FIXUP_TIME_META_KEY)?.is_some() {
        return Ok(());
    }
    log::info!("Running login fixup");
    let now = util::system_time_ms_i64(std::time::SystemTime::now());
    // Write it in advance. If something goes wrong, we don't want to
    // keep trying this over and over again.
    db.put_meta(crate::schema::LAST_FIXUP_TIME_META_KEY, &now)?;

    let records = db.get_all()?.into_iter().filter_map(|record| {
        let new_record = fix_login(&record);
        if new_record != record {
            Some(new_record)
        } else {
            None
        }
    });

    // Not bothering with a transaction since these changes should all
    // be improvements -- we dont want to roll back on failure.
    for rec in records {
        log::debug!("Applying change for record {}", rec.id);
        db.update(rec)?;
    }

    log::info!("Fixup finished");
    // Note: Arguably, we should dedupe here, but for now, we aren't, since we
    // aren't in a good position to resolve any conflicts we find.
    Ok(())
}


// `allow_non_origin` indicates if we're willing to take a non-origin if its a valid url but
// that the URL spec has defined as having an opaque origin (includes all schemes
// other than `"ftp" | "gopher" | "http" | "https" | "ws" | "wss"`)
pub fn try_fixup_origin_string(url_str: &str, allow_non_origin: bool, allow_empty_string: bool) -> Option<String> {
    let url_str = url_str
        .trim()
        .replace(|c| c == '\0' || c == '\r' || c == '\n', "");
    if url_str == "" || url_str == "." {
        return if allow_empty_string {
            Some("".into())
        } else {
            None
        };
    }
    let url = match Url::parse(&url_str) {
        Ok(v) => v,
        Err(_) => {
            // try again with only the parts we actually want, in case
            // some garbage is in the path or userinfo bits that we
            // ignore. We also remove spaces from the result, in desperation.
            let (prefix, hostport) = util::prefix_hostport(&url_str);
            let to_parse = (prefix.to_string() + hostport).replace(|c| c == ' ' || c == '\t', "");
            if to_parse.is_empty() && allow_empty_string {
                return Some("".into());
            }

            Url::parse(&to_parse).ok()?
        }
    };
    match url.origin() {
        // All schemes other than a few well known ones come through as opaque,
        // and stringify as "null" :|
        Origin::Opaque(_) => {
            if allow_non_origin {
                Some(url.to_string())
            } else {
                None
            }
        }
        tuple_origin => Some(tuple_origin.ascii_serialization()),
    }
}
