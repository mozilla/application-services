/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use failure::Fail;

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(
        display = "The `sync_status` column in DB has an illegal value: {}",
        _0
    )]
    BadSyncStatus(u8),

    #[fail(
        display = "Schema name {:?} does not match the collection name for this remerge database ({:?})",
        _0, _1
    )]
    SchemaNameMatchError(String, String),

    #[fail(display = "Invalid schema: {}", _0)]
    SchemaError(#[fail(cause)] crate::schema::error::SchemaError),

    #[fail(display = "Error parsing JSON data: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "Error executing SQL: {}", _0)]
    SqlError(#[fail(cause)] rusqlite::Error),

    #[fail(display = "Error parsing URL: {}", _0)]
    UrlParseError(#[fail(cause)] url::ParseError),
}

error_support::define_error! {
    ErrorKind {
        (JsonError, serde_json::Error),
        (SchemaError, crate::schema::error::SchemaError),
        (UrlParseError, url::ParseError),
        (SqlError, rusqlite::Error),
    }
}
