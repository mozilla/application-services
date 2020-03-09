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
//!   - remerge/collection-name
//!   - remerge/local-schema-version
//!   - remerge/native-schema-version
//!   - remerge/client-id
//!   - remerge/change-counter

use super::{bundle::ToLocalReason, meta, LocalRecord, NativeRecord, SchemaBundle};
use crate::schema::desc::RecordSchema;
use crate::schema::error::SchemaError;
use crate::error::*;
use crate::Guid;
use rusqlite::{Connection, named_params};
use std::sync::Arc;

pub(super) fn load_or_bootstrap(
    db: &Connection,
    native: super::NativeSchemaAndText<'_>,
) -> Result<(SchemaBundle, Guid)> {
    println!("URGHHH!! {:?}", meta::try_get::<String>(db, meta::COLLECTION_NAME));
    if let Some(name) = meta::try_get::<String>(db, meta::COLLECTION_NAME)? {
        let native = native.parsed;
        if name != native.name {
            throw!(ErrorKind::SchemaNameMatchError(native.name.clone(), name));
        }
        let local_ver: String = meta::get(db, meta::LOCAL_SCHEMA_VERSION)?;
        let native_ver: String = meta::get(db, meta::NATIVE_SCHEMA_VERSION)?;
        let client_id: sync_guid::Guid = meta::get(db, meta::OWN_CLIENT_ID)?;

        // XXX need to think about what to do if this fails! More generally, is
        // it sane to run validation on schemas already in the DB? If the answer
        // is yes, we should probably have more tests to ensure we never begin
        // rejecting a schema we previously considered valid!
        // let parsed = crate::schema::parse_from_string(&local_schema, false)?;
        let parsed = get_schema(&db, &local_ver)?;
        let previous_native = get_schema(&db, &native_ver)?;
        let previous_bundle = SchemaBundle {
            local: Arc::new(parsed.clone()),
            native: Arc::new(previous_native.clone()),
            collection_name: name.to_string(),
        };
        // println!("GOT HERE! {}, {}", native_ver, native.version.to_string());

        if native_ver != native.version.to_string() {
            // XXX migrate existing records here!
            let new_bundle = SchemaBundle {
                local: Arc::new(parsed),
                native: native.clone(),
                collection_name: name.to_string(),
            };
            migrate_records(&db, &previous_bundle, &new_bundle)?;
            let native_ver = semver::Version::parse(&*native_ver)
                .expect("previously-written version is no longer semver");
            if native.version < native_ver {
                throw!(ErrorKind::SchemaVersionWentBackwards(
                    native.version.to_string(),
                    native_ver.to_string()
                ));
            }
            meta::put(db, meta::NATIVE_SCHEMA_VERSION, &native.version.to_string())?;
        } else {
            if *native != previous_native {
                throw!(ErrorKind::SchemaChangedWithoutVersionBump(
                    native.version.to_string()
                ));
            }
        }
        let parsed = get_schema(&db, &local_ver)?;
        Ok((
            SchemaBundle {
                local: Arc::new(parsed),
                native: native.clone(),
                collection_name: name,
            },
            client_id,
        ))
    } else {
        bootstrap(db, native)
    }
}

pub(super) fn bootstrap(
    db: &Connection,
    native: super::NativeSchemaAndText<'_>,
) -> Result<(SchemaBundle, Guid)> {
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
    meta::put(db, meta::CHANGE_COUNTER, &1)?;
    Ok((
        SchemaBundle {
            collection_name: native.parsed.name.clone(),
            native: native.parsed.clone(),
            local: native.parsed.clone(),
        },
        guid,
    ))
}

fn get_schema(
    db: &Connection,
    schema_ver: &String
) -> Result<RecordSchema, SchemaError> {
    let native: String = db.query_row(
        "SELECT schema_text FROM remerge_schemas WHERE version = ?",
        rusqlite::params![schema_ver],
        |r| r.get(0),
    )
    .unwrap();

    crate::schema::parse_from_string(&*native, false)
}

fn migrate_records(
    db: &Connection,
    previous_native: &SchemaBundle,
    // local: &RecordSchema
    new_native: &SchemaBundle
) -> Result<()> {
    // Check if there are dedupe_on fields
    if !new_native.local.dedupe_on.is_empty() {
        unimplemented!("FIXME: migration");
    }

    // Get existing records with old schema version
    let mut stmt = db.prepare_cached("SELECT record_data FROM rec_mirror WHERE is_overridden = 0")?;
    let rows = stmt.query_and_then(rusqlite::NO_PARAMS, |row| -> Result<NativeRecord> {
        let r: LocalRecord = row.get("record_data")?;
        previous_native.local_to_native(&r)
    })?;
    let old_records: Vec<NativeRecord> = rows.collect::<Result<_>>().unwrap_or_default();
    // let mut new_records: Vec<NativeRecord> = Vec::new();

    for old_record in old_records {
        let mut new_record_data = crate::JsonObject::default();

        for new_record_field in &new_native.local.fields {
            let value = old_record.as_obj()[new_record_field.name.as_str()].clone();

            //  Apply defaults from the new schema where applicable.
            new_record_data.insert(new_record_field.clone().name, value);
        }
        // &new_records.push(NativeRecord::new_unchecked(new_record_data));
        let new_native_record = NativeRecord::new_unchecked(new_record_data);
        let (id, record) = new_native.native_to_local(&new_native_record, ToLocalReason::Creation)?;
    }
    // x Apply field restrictions from the new schema where applicable (min, max, etc. via the `field.validate` function).

    // TODO: Insert updated records into `rec_local` with the new schema version number in the `remerge_schema_version` field.


    // Remove records with old schema version
    db.execute_named(
        "UPDATE rec_mirror
         SET is_overridden = 1
         WHERE remerge_schema_version = :previous_ver",
        named_params! {
            ":previous_ver": new_native.local.version.to_string(),
        },
    )?;

    Ok(())
}
