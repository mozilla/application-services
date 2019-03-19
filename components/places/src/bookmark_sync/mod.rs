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
use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::SyncGuid;
use rusqlite::types::{ToSql, ToSqlOutput};
use rusqlite::Result as RusqliteResult;
use sql_support::ConnExt;

/// Sets up the syncable roots. All items in `moz_bookmarks_synced` descend
/// from these roots.
pub fn create_synced_bookmark_roots(db: &PlacesDb) -> Result<()> {
    // NOTE: This is called in a transaction.
    // XXX - not at all clear if these default values are OK!
    fn maybe_insert(
        db: &PlacesDb,
        guid: &SyncGuid,
        parent_guid: &Option<SyncGuid>,
        pos: u32,
    ) -> Result<()> {
        db.execute_named_cached(
            r#"INSERT OR IGNORE INTO moz_bookmarks_synced(
                guid, parentGuid, serverModified, needsMerge, kind,
                dateAdded, title, validity)
               VALUES(:guid, :parentGuid, :serverModified, :needsMerge, :kind,
                      :dateAdded, NULLIF(:title, ""), :validity)"#,
            &[
                (":guid", &guid),
                (":parentGuid", &parent_guid),
                (":serverModified", &0),
                (":needsMerge", &false),
                (":kind", &SyncedBookmarkKind::Folder),
                (":dateAdded", &0),
                (":title", &""),
                (":validity", &SyncedBookmarkValidity::Valid),
            ],
        )?;
        if let Some(parent_guid) = parent_guid {
            db.execute_named_cached(
                r#"INSERT OR IGNORE INTO moz_bookmarks_synced_structure(
                    guid, parentGuid, position)
                   VALUES(:guid, :parentGuid, :position)"#,
                &[
                    (":guid", &guid),
                    (":parentGuid", &parent_guid),
                    (":position", &pos),
                ],
            )?;
        }
        Ok(())
    }

    maybe_insert(db, &BookmarkRootGuid::Root.as_guid(), &None, 0)?;
    // does the order here matter?
    for (pos, user_root) in BookmarkRootGuid::user_roots().iter().enumerate() {
        maybe_insert(
            db,
            &user_root.as_guid(),
            &Some(BookmarkRootGuid::Root.as_guid()),
            pos as u32,
        )?;
    }
    Ok(())
}

impl BookmarkRootGuid {
    pub fn as_sync_record_id(&self) -> &'static str {
        match self {
            BookmarkRootGuid::Root => "places",
            BookmarkRootGuid::Menu => "menu",
            BookmarkRootGuid::Toolbar => "toolbar",
            BookmarkRootGuid::Unfiled => "unfiled",
            BookmarkRootGuid::Mobile => "mobile",
        }
    }

    pub fn from_sync_record_id(id: &str) -> Option<Self> {
        Some(match id {
            "places" => BookmarkRootGuid::Root,
            "menu" => BookmarkRootGuid::Menu,
            "toolbar" => BookmarkRootGuid::Toolbar,
            "unfiled" => BookmarkRootGuid::Unfiled,
            "mobile" => BookmarkRootGuid::Mobile,
            _ => return None,
        })
    }

    pub fn user_roots() -> Vec<BookmarkRootGuid> {
        vec![
            BookmarkRootGuid::Menu,
            BookmarkRootGuid::Toolbar,
            BookmarkRootGuid::Unfiled,
            BookmarkRootGuid::Mobile,
        ]
    }
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
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
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
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}
