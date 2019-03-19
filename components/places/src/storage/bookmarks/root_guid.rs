/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::SyncGuid;
use lazy_static::lazy_static;

/// Special GUIDs associated with bookmark roots.
/// It's guaranteed that the roots will always have these guids.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
#[repr(u8)]
pub enum BookmarkRootGuid {
    Root,
    Menu,
    Toolbar,
    Unfiled,
    Mobile,
}

lazy_static! {
    static ref GUIDS: [(BookmarkRootGuid, SyncGuid); 5] = [
        (
            BookmarkRootGuid::Root,
            SyncGuid(BookmarkRootGuid::Root.as_str().into())
        ),
        (
            BookmarkRootGuid::Menu,
            SyncGuid(BookmarkRootGuid::Menu.as_str().into())
        ),
        (
            BookmarkRootGuid::Toolbar,
            SyncGuid(BookmarkRootGuid::Toolbar.as_str().into())
        ),
        (
            BookmarkRootGuid::Unfiled,
            SyncGuid(BookmarkRootGuid::Unfiled.as_str().into())
        ),
        (
            BookmarkRootGuid::Mobile,
            SyncGuid(BookmarkRootGuid::Mobile.as_str().into())
        ),
    ];
}

impl BookmarkRootGuid {
    pub fn as_str(self) -> &'static str {
        match self {
            BookmarkRootGuid::Root => "root________",
            BookmarkRootGuid::Menu => "menu________",
            BookmarkRootGuid::Toolbar => "toolbar_____",
            BookmarkRootGuid::Unfiled => "unfiled_____",
            BookmarkRootGuid::Mobile => "mobile______",
        }
    }

    pub fn guid(self) -> &'static SyncGuid {
        &GUIDS[self as usize].1
    }

    pub fn as_guid(self) -> SyncGuid {
        self.guid().clone()
    }

    pub fn from_str(guid: &str) -> Option<Self> {
        GUIDS
            .iter()
            .find(|(_, sync_guid)| &sync_guid.0 == guid)
            .map(|(root, _)| *root)
    }

    pub fn from_guid(guid: &SyncGuid) -> Option<Self> {
        Self::from_str(&guid.0)
    }

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

impl From<BookmarkRootGuid> for SyncGuid {
    fn from(item: BookmarkRootGuid) -> SyncGuid {
        item.as_guid()
    }
}

// Allow comparisons between BookmarkRootGuid and SyncGuids
impl PartialEq<BookmarkRootGuid> for SyncGuid {
    fn eq(&self, other: &BookmarkRootGuid) -> bool {
        &self.0 == other.as_str()
    }
}

impl PartialEq<SyncGuid> for BookmarkRootGuid {
    fn eq(&self, other: &SyncGuid) -> bool {
        &other.0 == self.as_str()
    }
}

// Even if we have a reference to &SyncGuid
impl<'a> PartialEq<BookmarkRootGuid> for &'a SyncGuid {
    fn eq(&self, other: &BookmarkRootGuid) -> bool {
        &self.0 == other.as_str()
    }
}

impl<'a> PartialEq<&'a SyncGuid> for BookmarkRootGuid {
    fn eq(&self, other: &&'a SyncGuid) -> bool {
        &other.0 == self.as_str()
    }
}

// And between BookmarkRootGuid and &str
impl<'a> PartialEq<BookmarkRootGuid> for &'a str {
    fn eq(&self, other: &BookmarkRootGuid) -> bool {
        *self == other.as_str()
    }
}

impl<'a> PartialEq<&'a str> for BookmarkRootGuid {
    fn eq(&self, other: &&'a str) -> bool {
        self.as_str() == *other
    }
}
