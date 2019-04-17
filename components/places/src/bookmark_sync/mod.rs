/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod incoming;
pub mod record;
pub mod store;

#[cfg(test)]
mod tests;

use crate::db::PlacesDb;
use crate::error::*;
use crate::storage::bookmarks::{BookmarkRootGuid, USER_CONTENT_ROOTS};
use crate::types::SyncGuid;
use rusqlite::types::{ToSql, ToSqlOutput};
use rusqlite::Result as RusqliteResult;

/// Sets up the syncable roots. All items in `moz_bookmarks_synced` descend
/// from these roots.
pub fn create_synced_bookmark_roots(db: &PlacesDb) -> Result<()> {
    // NOTE: This is called in a transaction.
    fn maybe_insert(
        db: &PlacesDb,
        guid: &SyncGuid,
        parent_guid: &SyncGuid,
        pos: u32,
    ) -> Result<()> {
        db.execute_batch(&format!(
            "INSERT OR IGNORE INTO moz_bookmarks_synced(guid, parentGuid, kind)
             VALUES('{guid}', '{parent_guid}', {kind});

             INSERT OR IGNORE INTO moz_bookmarks_synced_structure(
                 guid, parentGuid, position)
             VALUES('{guid}', '{parent_guid}', {pos});",
            guid = guid.as_ref(),
            parent_guid = parent_guid.as_ref(),
            kind = SyncedBookmarkKind::Folder as u8,
            pos = pos
        ))?;
        Ok(())
    }

    // The Places root is its own parent, to ensure it's always in
    // `moz_bookmarks_synced_structure`.
    maybe_insert(
        db,
        &BookmarkRootGuid::Root.as_guid(),
        &BookmarkRootGuid::Root.as_guid(),
        0,
    )?;
    for (pos, user_root) in USER_CONTENT_ROOTS.iter().enumerate() {
        maybe_insert(
            db,
            &user_root.as_guid(),
            &BookmarkRootGuid::Root.as_guid(),
            pos as u32,
        )?;
    }
    Ok(())
}

/// Synced item kinds. These are stored in `moz_bookmarks_synced.kind` and match
/// the definitions in `mozISyncedBookmarksMerger`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum SyncedBookmarkKind {
    Bookmark = 1,  // KIND_BOOKMARK
    Query = 2,     // KIND_QUERY
    Folder = 3,    // KIND_FOLDER
    Livemark = 4,  // KIND_LIVEMARK
    Separator = 5, // KIND_SEPARATOR
}

impl SyncedBookmarkKind {
    #[inline]
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            1 => Ok(SyncedBookmarkKind::Bookmark),
            2 => Ok(SyncedBookmarkKind::Query),
            3 => Ok(SyncedBookmarkKind::Folder),
            4 => Ok(SyncedBookmarkKind::Livemark),
            5 => Ok(SyncedBookmarkKind::Separator),
            _ => Err(ErrorKind::UnsupportedSyncedBookmarkKind(v).into()),
        }
    }
}

impl From<SyncedBookmarkKind> for dogear::Kind {
    fn from(kind: SyncedBookmarkKind) -> dogear::Kind {
        match kind {
            SyncedBookmarkKind::Bookmark => dogear::Kind::Bookmark,
            SyncedBookmarkKind::Query => dogear::Kind::Query,
            SyncedBookmarkKind::Folder => dogear::Kind::Folder,
            SyncedBookmarkKind::Livemark => dogear::Kind::Livemark,
            SyncedBookmarkKind::Separator => dogear::Kind::Separator,
        }
    }
}

impl ToSql for SyncedBookmarkKind {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

/// Synced item validity states. These are stored in
/// `moz_bookmarks_synced.validity`, and match the definitions in
/// `mozISyncedBookmarksMerger`. In short:
/// * `Valid` means the record is valid and should be merged as usual.
/// * `Reupload` means a remote item can be fixed up and applied,
///    and should be reuploaded.
/// * `Replace` means a remote item isn't valid at all, and should either be
///    replaced with a valid local copy, or deleted if a valid local copy
///    doesn't exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum SyncedBookmarkValidity {
    Valid = 1,    // VALIDITY_VALID
    Reupload = 2, // VALIDITY_REUPLOAD
    Replace = 3,  // VALIDITY_REPLACE
}

impl SyncedBookmarkValidity {
    #[inline]
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            1 => Ok(SyncedBookmarkValidity::Valid),
            2 => Ok(SyncedBookmarkValidity::Reupload),
            3 => Ok(SyncedBookmarkValidity::Replace),
            _ => Err(ErrorKind::UnsupportedSyncedBookmarkValidity(v).into()),
        }
    }
}

impl From<SyncedBookmarkValidity> for dogear::Validity {
    fn from(validity: SyncedBookmarkValidity) -> dogear::Validity {
        match validity {
            SyncedBookmarkValidity::Valid => dogear::Validity::Valid,
            SyncedBookmarkValidity::Reupload => dogear::Validity::Reupload,
            SyncedBookmarkValidity::Replace => dogear::Validity::Replace,
        }
    }
}

impl ToSql for SyncedBookmarkValidity {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}
