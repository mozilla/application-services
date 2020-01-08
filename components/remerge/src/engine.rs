/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::storage::{db::RemergeDb, NativeRecord, NativeSchemaAndText, SchemaBundle};
use crate::Guid;
use std::convert::{TryFrom, TryInto};
use std::path::Path;

/// "Friendly" public api for using Remerge.
pub struct RemergeEngine {
    pub(crate) db: RemergeDb,
}

impl RemergeEngine {
    pub fn open(path: impl AsRef<Path>, schema_json: impl AsRef<str>) -> Result<Self> {
        let schema = NativeSchemaAndText::try_from(schema_json.as_ref())?;
        let conn = rusqlite::Connection::open(path.as_ref())?;
        let db = RemergeDb::with_connection(conn, schema)?;
        Ok(Self { db })
    }

    pub fn open_in_memory(schema_json: impl AsRef<str>) -> Result<Self> {
        let schema = NativeSchemaAndText::try_from(schema_json.as_ref())?;
        let conn = rusqlite::Connection::open_in_memory()?;
        let db = RemergeDb::with_connection(conn, schema)?;
        Ok(Self { db })
    }

    pub fn bundle(&self) -> &SchemaBundle {
        self.db.bundle()
    }

    pub fn list(&self) -> Result<Vec<NativeRecord>> {
        self.db.get_all()
    }

    pub fn exists(&self, id: impl AsRef<str>) -> Result<bool> {
        self.db.exists(id.as_ref())
    }

    pub fn get(&self, id: impl AsRef<str>) -> Result<Option<NativeRecord>> {
        self.db.get_by_id(id.as_ref())
    }

    pub fn delete(&self, id: impl AsRef<str>) -> Result<bool> {
        self.db.delete_by_id(id.as_ref())
    }

    pub fn update<R>(&self, rec: R) -> Result<()>
    where
        R: TryInto<NativeRecord>,
        Error: From<R::Error>,
    {
        self.db.update_record(&rec.try_into()?)
    }

    pub fn insert<R>(&self, rec: R) -> Result<Guid>
    where
        R: TryInto<NativeRecord>,
        Error: From<R::Error>,
    {
        self.db.create(&rec.try_into()?)
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::JsonValue;
    use serde_json::json;

    lazy_static::lazy_static! {
        pub static ref SCHEMA: String = json!({
            "version": "1.0.0",
            "name": "logins-example",
            "legacy": true,
            "fields": [
                {
                    "name": "id",
                    "type": "own_guid"
                },
                {
                    "name": "formSubmitUrl",
                    "type": "url",
                    "is_origin": true,
                    "local_name": "formActionOrigin"
                },
                {
                    "name": "httpRealm",
                    "type": "text",
                    "composite_root": "formSubmitUrl"
                },
                {
                    "name": "timesUsed",
                    "type": "integer",
                    "merge": "take_sum"
                },
                {
                    "name": "hostname",
                    "local_name": "origin",
                    "type": "url",
                    "is_origin": true,
                    "required": true
                },
                {
                    "name": "password",
                    "type": "text",
                    "required": true
                },
                {
                    "name": "username",
                    "type": "text"
                }
            ],
            "dedupe_on": [
                "username",
                "password",
                "hostname"
            ]
        }).to_string();
    }

    #[test]
    fn test_init() {
        let e: RemergeEngine = RemergeEngine::open_in_memory(&*SCHEMA).unwrap();
        assert_eq!(e.bundle().collection_name(), "logins-example");
    }

    #[test]
    fn test_insert() {
        let e: RemergeEngine = RemergeEngine::open_in_memory(&*SCHEMA).unwrap();
        let id = e
            .insert(json!({
                "username": "test",
                "password": "p4ssw0rd",
                "origin": "https://www.example.com",
                "formActionOrigin": "https://login.example.com",
            }))
            .unwrap();
        assert!(e.exists(&id).unwrap());
        let r = e.get(&id).unwrap().expect("should exist");

        let v: JsonValue = r.into_val();
        assert_eq!(v["id"], id.as_str());
        assert_eq!(v["username"], "test");
        assert_eq!(v["password"], "p4ssw0rd");
        assert_eq!(v["origin"], "https://www.example.com");
        assert_eq!(v["formActionOrigin"], "https://login.example.com");
    }

    #[test]
    fn test_list_delete() {
        let e: RemergeEngine = RemergeEngine::open_in_memory(&*SCHEMA).unwrap();
        let id = e
            .insert(json!({
                "username": "test",
                "password": "p4ssw0rd",
                "origin": "https://www.example.com",
                "formActionOrigin": "https://login.example.com",
            }))
            .unwrap();
        assert!(e.exists(&id).unwrap());

        e.get(&id).unwrap().expect("should exist");

        let id2 = e
            .insert(json!({
                "id": "abcd12349876",
                "username": "test2",
                "password": "p4ssw0rd0",
                "origin": "https://www.ex4mple.com",
                "httpRealm": "stuff",
            }))
            .unwrap();
        assert_eq!(id2, "abcd12349876");

        let l = e.list().unwrap();
        assert_eq!(l.len(), 2);
        assert!(l.iter().any(|r| r["id"] == id.as_str()));

        let v2 = l
            .iter()
            .find(|r| r["id"] == id2.as_str())
            .expect("should exist")
            .clone()
            .into_val();
        assert_eq!(v2["username"], "test2");
        assert_eq!(v2["password"], "p4ssw0rd0");
        assert_eq!(v2["origin"], "https://www.ex4mple.com");
        assert_eq!(v2["httpRealm"], "stuff");

        let del = e.delete(&id).unwrap();
        assert!(del);
        assert!(!e.exists(&id).unwrap());

        let l = e.list().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0]["id"], id2.as_str());
    }

    #[test]
    fn test_update() {
        let e: RemergeEngine = RemergeEngine::open_in_memory(&*SCHEMA).unwrap();
        let id = e
            .insert(json!({
                "username": "test",
                "password": "p4ssw0rd",
                "origin": "https://www.example.com",
                "formActionOrigin": "https://login.example.com",
            }))
            .unwrap();
        assert!(e.exists(&id).unwrap());
        let v = e.get(&id).unwrap().expect("should exist").into_val();
        assert_eq!(v["id"], id.as_str());
        assert_eq!(v["username"], "test");
        assert_eq!(v["password"], "p4ssw0rd");
        assert_eq!(v["origin"], "https://www.example.com");
        assert_eq!(v["formActionOrigin"], "https://login.example.com");

        e.update(json!({
            "id": id,
            "username": "test2",
            "password": "p4ssw0rd0",
            "origin": "https://www.ex4mple.com",
            "httpRealm": "stuff",
        }))
        .unwrap();

        let v = e
            .get(&id)
            .unwrap()
            .expect("should (still) exist")
            .into_val();
        assert_eq!(v["id"], id.as_str());
        assert_eq!(v["username"], "test2");
        assert_eq!(v["password"], "p4ssw0rd0");
        assert_eq!(v["origin"], "https://www.ex4mple.com");
        assert_eq!(v["httpRealm"], "stuff");
    }
}
