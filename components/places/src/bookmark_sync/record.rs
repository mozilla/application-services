/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, storage::bookmarks::BookmarkRootGuid, types::SyncGuid};
use serde::{de, ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

/// All possible fields that can appear in a bookmark record.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBookmarkItemRecord {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "parentid")]
    parent_id: Option<String>,
    #[serde(rename = "parentName")]
    parent_title: Option<String>,
    date_added: Option<i64>,

    // For bookmarks, queries, folders, and livemarks.
    title: Option<String>,

    // For bookmarks and queries.
    #[serde(rename = "bmkUri")]
    url: Option<String>,

    // For bookmarks only.
    keyword: Option<String>,
    tags: Option<Vec<String>>,

    // For queries only.
    #[serde(rename = "folderName")]
    tag_folder_name: Option<String>,

    // For folders only.
    children: Option<Vec<SyncGuid>>,

    // For livemarks only.
    #[serde(rename = "feedUri")]
    feed_url: Option<String>,
    #[serde(rename = "siteUri")]
    site_url: Option<String>,

    // For separators only.
    #[serde(rename = "pos")]
    position: Option<i64>,
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct BookmarkRecord {
    // Note that `SyncGuid` does not check for validity, which is what we
    // want. If the bookmark has an invalid GUID, we'll make a new one.
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    pub parent_title: Option<String>,
    pub date_added: Option<i64>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub keyword: Option<String>,
    pub tags: Vec<String>,
}

impl From<BookmarkRecord> for BookmarkItemRecord {
    fn from(b: BookmarkRecord) -> BookmarkItemRecord {
        BookmarkItemRecord::Bookmark(b)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct QueryRecord {
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    pub parent_title: Option<String>,
    pub date_added: Option<i64>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub tag_folder_name: Option<String>,
}

impl From<QueryRecord> for BookmarkItemRecord {
    fn from(q: QueryRecord) -> BookmarkItemRecord {
        BookmarkItemRecord::Query(q)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct FolderRecord {
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    pub parent_title: Option<String>,
    pub date_added: Option<i64>,
    pub title: Option<String>,
    pub children: Vec<SyncGuid>,
}

impl From<FolderRecord> for BookmarkItemRecord {
    fn from(f: FolderRecord) -> BookmarkItemRecord {
        BookmarkItemRecord::Folder(f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct LivemarkRecord {
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    pub parent_title: Option<String>,
    pub date_added: Option<i64>,
    pub title: Option<String>,
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
}

impl From<LivemarkRecord> for BookmarkItemRecord {
    fn from(l: LivemarkRecord) -> BookmarkItemRecord {
        BookmarkItemRecord::Livemark(l)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct SeparatorRecord {
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    pub parent_title: Option<String>,
    pub date_added: Option<i64>,
    // Not used on newer clients, but can be used to detect parent-child
    // position disagreements. Older clients use this for deduping.
    pub position: Option<i64>,
}

impl From<SeparatorRecord> for BookmarkItemRecord {
    fn from(s: SeparatorRecord) -> BookmarkItemRecord {
        BookmarkItemRecord::Separator(s)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub enum BookmarkItemRecord {
    Tombstone(SyncGuid),
    Bookmark(BookmarkRecord),
    Query(QueryRecord),
    Folder(FolderRecord),
    Livemark(LivemarkRecord),
    Separator(SeparatorRecord),
}

impl BookmarkItemRecord {
    pub fn from_payload(payload: sync15::Payload) -> Result<Self> {
        let guid = payload.id.clone();
        let record = if payload.is_tombstone() {
            BookmarkItemRecord::Tombstone(guid.into())
        } else {
            payload.into_record()?
        };
        Ok(record)
    }
}

impl Serialize for BookmarkItemRecord {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BookmarkItemRecord", 2)?;
        match self {
            BookmarkItemRecord::Tombstone(guid) => {
                state.serialize_field("id", guid_to_id(&guid))?;
                state.serialize_field("deleted", &true)?;
            }
            BookmarkItemRecord::Bookmark(b) => {
                state.serialize_field("id", guid_to_id(&b.guid))?;
                state.serialize_field("type", "bookmark")?;
                state.serialize_field("parentid", &b.parent_guid.as_ref().map(guid_to_id))?;
                // `hasDupe` always defaults to `true`, to prevent older
                // Desktops from applying their deduping logic (bug 1408551).
                state.serialize_field("hasDupe", &true)?;
                state.serialize_field("parentName", &b.parent_title)?;
                state.serialize_field("dateAdded", &b.date_added)?;
                state.serialize_field("title", &b.title)?;
                state.serialize_field("bmkUri", &b.url)?;
                if let Some(ref keyword) = &b.keyword {
                    state.serialize_field("keyword", keyword)?;
                }
                if !b.tags.is_empty() {
                    state.serialize_field("tags", &b.tags)?;
                }
            }
            BookmarkItemRecord::Query(q) => {
                state.serialize_field("id", guid_to_id(&q.guid))?;
                state.serialize_field("type", "query")?;
                state.serialize_field("parentid", &q.parent_guid.as_ref().map(guid_to_id))?;
                state.serialize_field("hasDupe", &true)?;
                state.serialize_field("parentName", &q.parent_title)?;
                state.serialize_field("dateAdded", &q.date_added)?;
                state.serialize_field("title", &q.title)?;
                state.serialize_field("bmkUri", &q.url)?;
                state.serialize_field("folderName", &q.tag_folder_name)?;
            }
            BookmarkItemRecord::Folder(f) => {
                state.serialize_field("id", guid_to_id(&f.guid))?;
                state.serialize_field("type", "folder")?;
                state.serialize_field("parentid", &f.parent_guid.as_ref().map(guid_to_id))?;
                state.serialize_field("hasDupe", &true)?;
                state.serialize_field("parentName", &f.parent_title)?;
                state.serialize_field("dateAdded", &f.date_added)?;
                state.serialize_field("title", &f.title)?;
                state.serialize_field("children", &f.children)?;
            }
            BookmarkItemRecord::Livemark(l) => {
                state.serialize_field("id", guid_to_id(&l.guid))?;
                state.serialize_field("type", "livemark")?;
                state.serialize_field("parentid", &l.parent_guid.as_ref().map(guid_to_id))?;
                state.serialize_field("hasDupe", &true)?;
                state.serialize_field("parentName", &l.parent_title)?;
                state.serialize_field("dateAdded", &l.date_added)?;
                state.serialize_field("title", &l.title)?;
                state.serialize_field("feedUri", &l.feed_url)?;
                state.serialize_field("siteUri", &l.site_url)?;
            }
            BookmarkItemRecord::Separator(s) => {
                state.serialize_field("id", guid_to_id(&s.guid))?;
                state.serialize_field("type", "separator")?;
                state.serialize_field("parentid", &s.parent_guid.as_ref().map(guid_to_id))?;
                state.serialize_field("hasDupe", &true)?;
                state.serialize_field("parentName", &s.parent_title)?;
                state.serialize_field("dateAdded", &s.date_added)?;
                state.serialize_field("pos", &s.position)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for BookmarkItemRecord {
    fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Boilerplate to translate a synced bookmark record into a typed
        // record.
        let raw = RawBookmarkItemRecord::deserialize(d)?;
        match raw.kind.as_str() {
            "bookmark" => {
                return Ok(BookmarkRecord {
                    guid: id_to_guid(raw.id),
                    parent_guid: raw.parent_id.map(id_to_guid),
                    parent_title: raw.parent_title,
                    date_added: raw.date_added,
                    title: raw.title,
                    url: raw.url,
                    keyword: raw.keyword,
                    tags: raw.tags.unwrap_or_default(),
                }
                .into());
            }
            "query" => {
                return Ok(QueryRecord {
                    guid: id_to_guid(raw.id),
                    parent_guid: raw.parent_id.map(id_to_guid),
                    parent_title: raw.parent_title,
                    date_added: raw.date_added,
                    title: raw.title,
                    url: raw.url,
                    tag_folder_name: raw.tag_folder_name,
                }
                .into());
            }
            "folder" => {
                return Ok(FolderRecord {
                    guid: id_to_guid(raw.id),
                    parent_guid: raw.parent_id.map(id_to_guid),
                    parent_title: raw.parent_title,
                    date_added: raw.date_added,
                    title: raw.title,
                    children: raw.children.unwrap_or_default(),
                }
                .into());
            }
            "livemark" => {
                return Ok(LivemarkRecord {
                    guid: id_to_guid(raw.id),
                    parent_guid: raw.parent_id.map(id_to_guid),
                    parent_title: raw.parent_title,
                    date_added: raw.date_added,
                    title: raw.title,
                    feed_url: raw.feed_url,
                    site_url: raw.site_url,
                }
                .into());
            }
            "separator" => {
                return Ok(SeparatorRecord {
                    guid: id_to_guid(raw.id),
                    parent_guid: raw.parent_id.map(id_to_guid),
                    parent_title: raw.parent_title,
                    date_added: raw.date_added,
                    position: raw.position,
                }
                .into());
            }
            _ => {}
        }
        // We can't meaningfully merge or round-trip item kinds that we don't
        // support, so fail deserialization.
        Err(de::Error::unknown_variant(
            raw.kind.as_str(),
            &["bookmark", "query", "folder", "livemark", "separator"],
        ))
    }
}

/// Converts a Sync bookmark record ID to a Places GUID. Sync record IDs are
/// identical to Places GUIDs for all items except roots.
#[inline]
pub fn id_to_guid(id: String) -> SyncGuid {
    BookmarkRootGuid::from_sync_record_id(&id)
        .map(|g| g.as_guid())
        .unwrap_or_else(|| id.into())
}

/// Converts a Places GUID to a a Sync bookmark record ID.
#[inline]
pub fn guid_to_id(guid: &SyncGuid) -> &str {
    BookmarkRootGuid::from_guid(guid)
        .map(|g| g.as_sync_record_id())
        .unwrap_or_else(|| guid.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Error};

    #[test]
    fn test_invalid_record_type() {
        let r: std::result::Result<BookmarkItemRecord, Error> =
            serde_json::from_value(json!({"id": "whatever", "type" : "unknown-type"}));
        let e = r.unwrap_err();
        assert!(e.is_data());
        // I guess is good enough to check we are hitting what we expect.
        assert!(e.to_string().contains("unknown-type"));
    }
}
