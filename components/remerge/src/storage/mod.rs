/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod bootstrap;
pub mod db;
mod meta;
pub mod schema;

use crate::schema::RecordSchema;
use std::sync::Arc;

/// Basically just input for initializing the database.
///
/// XXX Ideally this would just be Arc<RecordSchema>, but during bootstrapping
/// we need to insert the schema into the database, which requires that we have
/// the serialized form. Eventually we should allow turning a RecordSchema back
/// into a JSON (e.g. raw) schema. (We don't really want to support
/// serializing/deserializing a RecordSchema directly, since we already have a
/// stable serialization format for schemas, and don't need two).
///
/// Initialize with TryFrom.
#[derive(Clone)]
pub struct NativeSchemaInfo<'a> {
    pub parsed: Arc<RecordSchema>,
    pub source: &'a str,
}

impl<'a> std::convert::TryFrom<&'a str> for NativeSchemaInfo<'a> {
    type Error = crate::schema::SchemaError;
    fn try_from(s: &'a str) -> std::result::Result<Self, Self::Error> {
        let schema = crate::schema::parse_from_string(s, false)?;
        Ok(Self {
            parsed: Arc::new(schema),
            source: s,
        })
    }
}
