/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    BookmarkPosition, BookmarkUpdateInfo, InvalidPlaceInfo, UpdatableBookmark, UpdatableFolder,
    UpdatableItem, UpdatableSeparator, UpdateTreeLocation,
};

use crate::error::Result;
use crate::types::BookmarkType;
use sync_guid::Guid as SyncGuid;
use url::Url;

impl BookmarkUpdateInfo {
    /// The functions exposed over the FFI use the same type for all inserts.
    /// This function converts that into the type our update API uses.
    pub fn into_updatable(self, ty: BookmarkType) -> Result<(SyncGuid, UpdatableItem)> {
        // Check the things that otherwise would be enforced by the type system.

        if self.title.is_some() && ty == BookmarkType::Separator {
            return Err(InvalidPlaceInfo::IllegalChange("title", ty).into());
        }

        if self.url.is_some() && ty != BookmarkType::Bookmark {
            return Err(InvalidPlaceInfo::IllegalChange("url", ty).into());
        }

        let location = match (self.parent_guid, self.position) {
            (None, None) => UpdateTreeLocation::None,
            (None, Some(pos)) => UpdateTreeLocation::Position {
                pos: BookmarkPosition::Specific { pos },
            },
            (Some(parent_guid), pos) => UpdateTreeLocation::Parent {
                guid: parent_guid,
                pos: pos.map_or(BookmarkPosition::Append, |p| BookmarkPosition::Specific {
                    pos: p,
                }),
            },
        };

        let updatable = match ty {
            BookmarkType::Bookmark => UpdatableItem::Bookmark {
                b: UpdatableBookmark {
                    location,
                    title: self.title,
                    url: self.url.map(|u| Url::parse(&u)).transpose()?,
                },
            },
            BookmarkType::Separator => UpdatableItem::Separator {
                s: UpdatableSeparator { location },
            },
            BookmarkType::Folder => UpdatableItem::Folder {
                f: UpdatableFolder {
                    location,
                    title: self.title,
                },
            },
        };

        Ok((self.guid, updatable))
    }
}
