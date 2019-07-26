/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use logins::PasswordEngine;
use logins::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use sync_guid::Guid;
use tempfile::tempdir;

fn empty_fennec_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("./fennec_schema.sql"))?;
    Ok(conn)
}

#[derive(Clone, Debug)]
struct FennecLogin {
    hostname: String,
    http_realm: Option<String>,
    form_submit_url: Option<String>,
    username_field: String,
    password_field: String,
    encrypted_username: String,
    encrypted_password: String,
    guid: Guid,
    encryption_type: u8,
    time_created: Option<u32>,
    time_last_used: Option<u32>,
    time_password_changed: Option<u32>,
    times_used: Option<u16>,
}

impl FennecLogin {
    fn insert_into_db(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(&
            "INSERT OR IGNORE INTO logins(hostname, httpRealm, formSubmitURL, usernameField, passwordField,
                                          encryptedUsername, encryptedPassword, guid, encType, timeCreated,
                                          timeLastUsed, timePasswordChanged, timesUsed)
             VALUES (:hostname, :httpRealm, :formSubmitURL, :usernameField, :passwordField,
                     :encryptedUsername, :encryptedPassword, :guid, :encType, :timeCreated,
                     :timeLastUsed, :timePasswordChanged, :timesUsed)"
        )?;
        stmt.execute_named(rusqlite::named_params! {
             ":hostname": self.hostname,
             ":httpRealm": self.http_realm,
             ":formSubmitURL": self.form_submit_url,
             ":usernameField": self.username_field,
             ":passwordField": self.password_field,
             ":encryptedUsername": self.encrypted_username,
             ":encryptedPassword": self.encrypted_password,
             ":guid": self.guid,
             ":encType": self.encryption_type,
             ":timeCreated": self.time_created,
             ":timeLastUsed": self.time_last_used,
             ":timePasswordChanged": self.time_password_changed,
             ":timesUsed": self.times_used
        })?;
        Ok(())
    }
}

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Helps debugging to use these instead of actually random ones.
fn next_guid() -> Guid {
    let c = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let v = format!("test{}_______", c);
    let s = &v[..12];
    Guid::from(s)
}

impl Default for FennecLogin {
    fn default() -> Self {
        Self {
            hostname: "https://foo.bar".to_string(),
            http_realm: None,
            form_submit_url: None,
            username_field: "login".to_string(),
            password_field: "pwd".to_string(),
            encrypted_username: "bobo".to_string(),
            encrypted_password: "tron".to_string(),
            guid: next_guid(),
            encryption_type: 0,
            time_created: Some(1),
            time_last_used: Some(1),
            time_password_changed: Some(1),
            times_used: Some(1),
        }
    }
}

fn insert_logins(conn: &Connection, logins: &[FennecLogin]) -> Result<()> {
    for l in logins {
        l.insert_into_db(conn)?;
    }
    Ok(())
}

#[test]
fn test_import() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let fennec_path = tmpdir.path().join("browser.db");
    let fennec_db = empty_fennec_db(&fennec_path)?;

    let logins = [
        FennecLogin {
            http_realm: Some("bobo".to_owned()),
            ..Default::default()
        },
        // Worst case scenario, a bunch of NULL values.
        FennecLogin {
            form_submit_url: Some("tron".to_owned()),
            time_created: None,
            time_last_used: None,
            time_password_changed: None,
            times_used: None,
            ..Default::default()
        },
        // Both httpRealm and formSubmitURL are NOT NULL which is illegal.
        FennecLogin {
            http_realm: Some("https://foo.bar".to_owned()),
            form_submit_url: Some("https://foo.bar".to_owned()),
            ..Default::default()
        },
        // Both httpRealm and formSubmitURL are NULL which is also illegal.
        FennecLogin {
            ..Default::default()
        },
    ];
    insert_logins(&fennec_db, &logins)?;

    let engine = PasswordEngine::new(tmpdir.path().join("logins.sqlite"), None)?;
    logins::import::import_fennec_logins(&engine, fennec_path)?;
    // Uncomment the following to debug with cargo test -- --nocapture.
    // println!(
    //     "Logins DB Path: {}",
    //     tmpdir.path().join("logins.sqlite").to_str().unwrap()
    // );
    // ::std::process::exit(0);

    Ok(())
}
