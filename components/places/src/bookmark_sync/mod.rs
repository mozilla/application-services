/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod merger;
pub mod record;
pub mod store;

use crate::db::PlacesDb;
use crate::error::*;
use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::{SyncGuid, SyncedBookmarkKind, SyncedBookmarkValidity};
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
