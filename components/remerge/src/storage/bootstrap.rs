/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module is concerned mainly with initializing the schema and metadata
//! tables in the database. Specifically it has to handle the following cases
//!
//! ## First time initialization
//!
//! - Must insert the provided native schema into schemas table
//! - Must populate the metadata keys with their initial values. Specifically:
//!   - collection-name
//!   - local-schema-version
//!   - native-schema-version
//!

use super::meta;
use crate::error::*;
use crate::schema::RecordSchema;

use rusqlite::Connection;
use std::sync::Arc;

#[derive(Clone)]
pub struct RemergeInfo {
    collection_name: String,
    native: Arc<RecordSchema>,
    local: Arc<RecordSchema>,
}

pub(super) fn load_or_bootstrap(
    db: &Connection,
    native: super::NativeSchemaInfo<'_>,
) -> Result<RemergeInfo> {
    if let Some(name) = meta::try_get::<String>(db, meta::COLLECTION_NAME)? {
        let native = native.parsed;
        if name != native.name {
            throw!(ErrorKind::SchemaNameMatchError(
                native.name.clone(),
                name.clone()
            ));
        }
        let local_ver: String = meta::get(db, meta::LOCAL_SCHEMA_VERSION)?;
        let native_ver: String = meta::get(db, meta::NATIVE_SCHEMA_VERSION)?;

        if native_ver != native.version.to_string() {
            // XXX migrate existing records here!
            // XXX Ensure we only move native version forward and not backwards!
            meta::put(db, meta::NATIVE_SCHEMA_VERSION, &native.version.to_string())?;
        }
        let local_schema: String = db.query_row(
            "SELECT schema_text FROM remerge_schemas WHERE version = ? LIMIT 1",
            rusqlite::params![local_ver],
            |r| r.get(0),
        )?;
        // XXX need to think about what to do if this fails! More generally, is
        // it sane to run validation on schemas already in the DB? If the answer
        // is yes, we should probably have more tests to ensure we never begin
        // rejecting a schema we previously considered valid!
        let parsed = crate::schema::parse_from_string(&local_schema, false)?;
        Ok(RemergeInfo {
            local: Arc::new(parsed),
            native,
            collection_name: name,
        })
    } else {
        bootstrap(db, native)
    }
}

pub(super) fn bootstrap(
    db: &Connection,
    native: super::NativeSchemaInfo<'_>,
) -> Result<RemergeInfo> {
    let guid = sync_guid::Guid::random();
    meta::put(db, meta::OWN_CLIENT_ID, &guid)?;
    let sql = "
        INSERT INTO remerge_schemas (is_legacy, version, required_version, schema_text)
        VALUES (:legacy, :version, :req_version, :text)
    ";
    let ver_str = native.parsed.version.to_string();
    db.execute_named(
        sql,
        rusqlite::named_params! {
            ":legacy": native.parsed.legacy,
            ":version": ver_str,
            ":req_version": native.parsed.required_version.to_string(),
            ":text": native.source,
        },
    )?;
    meta::put(db, meta::LOCAL_SCHEMA_VERSION, &ver_str)?;
    meta::put(db, meta::NATIVE_SCHEMA_VERSION, &ver_str)?;
    meta::put(db, meta::COLLECTION_NAME, &native.parsed.name)?;
    Ok(RemergeInfo {
        collection_name: native.parsed.name.clone(),
        native: native.parsed.clone(),
        local: native.parsed.clone(),
    })
}
