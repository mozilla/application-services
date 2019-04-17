/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::record::{
    BookmarkItemRecord, BookmarkRecord, BookmarkRecordId, FolderRecord, LivemarkRecord,
    QueryRecord, SeparatorRecord,
};
use super::{SyncedBookmarkKind, SyncedBookmarkValidity};
use crate::error::*;
use crate::storage::{
    bookmarks::maybe_truncate_title,
    tags::{validate_tag, ValidatedTag},
    URL_LENGTH_MAX,
};
use crate::types::SyncGuid;
use rusqlite::Connection;
use sql_support::{self, ConnExt};
use std::iter;
use sync15::ServerTimestamp;
use url::Url;

// From Desktop's Ci.nsINavHistoryQueryOptions, but we define it as a str
// as that's how we use it here.
const RESULTS_AS_TAG_CONTENTS: &str = "7";

/// Manages the application of incoming records into the moz_bookmarks_synced
/// and related tables.
pub struct IncomingApplicator<'a> {
    db: &'a Connection,
}

impl<'a> IncomingApplicator<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn apply_payload(
        &self,
        payload: sync15::Payload,
        timestamp: ServerTimestamp,
    ) -> Result<()> {
        if payload.is_tombstone() {
            self.store_incoming_tombstone(
                timestamp,
                BookmarkRecordId::from_payload_id(payload.id).as_guid(),
            )?;
        } else {
            let item: BookmarkItemRecord = payload.into_record()?;
            match item {
                BookmarkItemRecord::Bookmark(b) => self.store_incoming_bookmark(timestamp, b)?,
                BookmarkItemRecord::Query(q) => self.store_incoming_query(timestamp, q)?,
                BookmarkItemRecord::Folder(f) => self.store_incoming_folder(timestamp, f)?,
                BookmarkItemRecord::Livemark(l) => self.store_incoming_livemark(timestamp, l)?,
                BookmarkItemRecord::Separator(s) => self.store_incoming_sep(timestamp, s)?,
            }
        }
        Ok(())
    }

    fn store_incoming_bookmark(&self, modified: ServerTimestamp, b: BookmarkRecord) -> Result<()> {
        let url = match self.maybe_store_href(b.url.as_ref()) {
            Ok(url) => (Some(url.into_string())),
            Err(e) => {
                log::warn!("Incoming bookmark has an invalid URL: {:?}", e);
                None
            }
        };
        let tags = b.tags.iter().map(|t| validate_tag(t));
        let validity = if url.is_none() {
            // The bookmark has an invalid URL, so we can't apply it.
            SyncedBookmarkValidity::Replace
        } else if tags.clone().all(|t| t.is_original()) {
            // The bookmark has a valid URL and all original tags, so we can
            // apply it as-is.
            SyncedBookmarkValidity::Valid
        } else {
            // The bookmark has a valid URL, but invalid or normalized tags. We
            // can apply it, but should also reupload it with the new tags.
            SyncedBookmarkValidity::Reupload
        };
        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, keyword, validity, placeId)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, NULLIF(:title, ""), :keyword, :validity,
                      CASE WHEN :url ISNULL
                      THEN NULL
                      ELSE (SELECT id FROM moz_places
                            WHERE url_hash = hash(:url) AND
                            url = :url)
                      END
                      )"#,
            &[
                (":guid", &b.record_id.as_guid().as_ref()),
                (":parentGuid", &b.parent_record_id.as_ref().map(BookmarkRecordId::as_guid)),
                (":serverModified", &(modified.as_millis() as i64)),
                (":kind", &SyncedBookmarkKind::Bookmark),
                (":dateAdded", &b.date_added),
                (":title", &maybe_truncate_title(&b.title)),
                (":keyword", &b.keyword),
                (":validity", &validity),
                (":url", &url),
            ],
        )?;
        for t in tags {
            match t {
                ValidatedTag::Invalid(ref t) => {
                    log::trace!("Ignoring invalid tag on incoming bookmark: {:?}", t);
                    continue;
                }
                ValidatedTag::Normalized(ref t) | ValidatedTag::Original(ref t) => {
                    self.db.execute_named_cached(
                        "INSERT OR IGNORE INTO moz_tags(tag, lastModified)
                         VALUES(:tag, now())",
                        &[(":tag", t)],
                    )?;
                    self.db.execute_named_cached(
                        "INSERT INTO moz_bookmarks_synced_tag_relation(itemId, tagId)
                         VALUES((SELECT id FROM moz_bookmarks_synced
                                 WHERE guid = :guid),
                                (SELECT id FROM moz_tags
                                 WHERE tag = :tag))",
                        &[(":guid", &b.record_id.as_guid().as_ref()), (":tag", t)],
                    )?;
                }
            };
        }
        Ok(())
    }

    fn store_incoming_folder(&self, modified: ServerTimestamp, f: FolderRecord) -> Result<()> {
        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, NULLIF(:title, ""))"#,
            &[
                (":guid", &f.record_id.as_guid().as_ref()),
                (":parentGuid", &f.parent_record_id.as_ref().map(BookmarkRecordId::as_guid)),
                (":serverModified", &(modified.as_millis() as i64)),
                (":kind", &SyncedBookmarkKind::Folder),
                (":dateAdded", &f.date_added),
                (":title", &maybe_truncate_title(&f.title)),
            ],
        )?;
        sql_support::each_sized_chunk(
            &f.children,
            // -1 because we want to leave an extra binding parameter (`?1`)
            // for the folder's GUID.
            sql_support::default_max_variable_number() - 1,
            |chunk, offset| -> Result<()> {
                let sql = format!(
                    "INSERT INTO moz_bookmarks_synced_structure(guid, parentGuid, position)
                     VALUES {}",
                    // Builds a fragment like `(?2, ?1, 0), (?3, ?1, 1), ...`,
                    // where ?1 is the folder's GUID, [?2, ?3] are the first and
                    // second child GUIDs (SQLite binding parameters index
                    // from 1), and [0, 1] are the positions. This lets us store
                    // the folder's children using as few statements as
                    // possible.
                    sql_support::repeat_display(chunk.len(), ",", |index, f| {
                        // Each child's position is its index in `f.children`;
                        // that is, the `offset` of the current chunk, plus the
                        // child's `index` within the chunk.
                        let position = offset + index;
                        write!(f, "(?{}, ?1, {})", index + 2, position)
                    })
                );
                self.db.execute(
                    &sql,
                    iter::once(&f.record_id)
                        .chain(chunk.iter())
                        .map(|record_id| record_id.as_guid().as_ref()),
                )?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn store_incoming_tombstone(&self, modified: ServerTimestamp, guid: &SyncGuid) -> Result<()> {
        self.db.execute_named_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge,
                                               dateAdded, isDeleted)
             VALUES(:guid, NULL, :serverModified, 1, 0, 1)",
            &[
                (":guid", guid),
                (":serverModified", &(modified.as_millis() as i64)),
            ],
        )?;
        Ok(())
    }

    fn determine_query_url_and_validity(
        &self,
        q: &QueryRecord,
        url: Url,
    ) -> Result<(Option<Url>, SyncedBookmarkValidity)> {
        // wow - this  is complex, but markh is struggling to see how to
        // improve it
        let (maybe_url, validity) = {
            // If the URL has `type={RESULTS_AS_TAG_CONTENTS}` then we
            // rewrite the URL as `place:tag=...`
            // Sadly we can't use `url.query_pairs()` here as the format of
            // the url is, eg, `place:type=7` - ie, the "params" are actually
            // the path portion of the URL.
            let parse = url::form_urlencoded::parse(&url.path().as_bytes());
            if parse
                .clone()
                .any(|(k, v)| k == "type" && v == RESULTS_AS_TAG_CONTENTS)
            {
                if let Some(tag_folder_name) = &q.tag_folder_name {
                    validate_tag(tag_folder_name)
                        .ensure_valid()
                        .and_then(|tag| Ok(Url::parse(&format!("place:tag={}", tag))?))
                        .map(|url| (Some(url), SyncedBookmarkValidity::Reupload))
                        .unwrap_or((None, SyncedBookmarkValidity::Replace))
                } else {
                    (None, SyncedBookmarkValidity::Replace)
                }
            } else {
                // If we have `folder=...` the folder value is a row_id
                // from desktop, so useless to us - so we append `&excludeItems=1`
                // if it isn't already there.
                if parse.clone().any(|(k, _)| k == "folder") {
                    if parse.clone().any(|(k, v)| k == "excludeItems" && v == "1") {
                        (Some(url), SyncedBookmarkValidity::Valid)
                    } else {
                        // need to add excludeItems, and I guess we should do
                        // it properly without resorting to string manipulation...
                        let tail = url::form_urlencoded::Serializer::new(String::new())
                            .extend_pairs(parse.clone())
                            .append_pair("excludeItems", "1")
                            .finish();
                        (
                            Some(Url::parse(&format!("place:{}", tail))?),
                            SyncedBookmarkValidity::Reupload,
                        )
                    }
                } else {
                    // it appears to be fine!
                    (Some(url), SyncedBookmarkValidity::Valid)
                }
            }
        };
        Ok(match self.maybe_store_url(maybe_url) {
            Ok(url) => (Some(url), validity),
            Err(e) => {
                log::warn!(
                    "query {} has invalid URL '{:?}': {:?}",
                    q.record_id.as_guid(),
                    q.url,
                    e
                );
                (None, SyncedBookmarkValidity::Replace)
            }
        })
    }

    fn store_incoming_query(&self, modified: ServerTimestamp, q: QueryRecord) -> Result<()> {
        let (url, validity) = match q.url.as_ref().and_then(|href| Url::parse(href).ok()) {
            Some(url) => self.determine_query_url_and_validity(&q, url)?,
            None => {
                log::warn!(
                    "query {} has invalid URL '{:?}'",
                    q.record_id.as_guid(),
                    q.url
                );
                (None, SyncedBookmarkValidity::Replace)
            }
        };

        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, validity, placeId)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, NULLIF(:title, ""), :validity,
                      (SELECT id FROM moz_places
                            WHERE url_hash = hash(:url) AND
                            url = :url
                      )
                     )"#,
            &[
                (":guid", &q.record_id.as_guid().as_ref()),
                (":parentGuid", &q.parent_record_id.as_ref().map(BookmarkRecordId::as_guid)),
                (":serverModified", &(modified.as_millis() as i64)),
                (":kind", &SyncedBookmarkKind::Query),
                (":dateAdded", &q.date_added),
                (":title", &maybe_truncate_title(&q.title)),
                (":validity", &validity),
                (":url", &url.map(Url::into_string))
            ],
        )?;
        Ok(())
    }

    fn store_incoming_livemark(&self, modified: ServerTimestamp, l: LivemarkRecord) -> Result<()> {
        // livemarks don't store a reference to the place, so we validate it manually.
        fn validate_href(h: Option<String>, guid: &SyncGuid, what: &str) -> Option<String> {
            match h {
                Some(h) => match Url::parse(&h) {
                    Ok(url) => {
                        let s = url.to_string();
                        if s.len() > URL_LENGTH_MAX {
                            log::warn!("Livemark {} has a {} URL which is too long", &guid, what);
                            None
                        } else {
                            Some(s)
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Livemark {} has an invalid {} URL '{}': {:?}",
                            &guid,
                            what,
                            h,
                            e
                        );
                        None
                    }
                },
                None => {
                    log::warn!("Livemark {} has no {} URL", &guid, what);
                    None
                }
            }
        }
        let feed_url = validate_href(l.feed_url, &l.record_id.as_guid(), "feed");
        let site_url = validate_href(l.site_url, &l.record_id.as_guid(), "site");
        let validity = if feed_url.is_some() {
            SyncedBookmarkValidity::Valid
        } else {
            SyncedBookmarkValidity::Replace
        };
        self.db.execute_named_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                               dateAdded, title, feedURL, siteURL, validity)
             VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                    :dateAdded, :title, :feedUrl, :siteUrl, :validity)",
            &[
                (":guid", &l.record_id.as_guid().as_ref()),
                (
                    ":parentGuid",
                    &l.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &(modified.as_millis() as i64)),
                (":kind", &SyncedBookmarkKind::Livemark),
                (":dateAdded", &l.date_added),
                (":title", &l.title),
                (":feedUrl", &feed_url),
                (":siteUrl", &site_url),
                (":validity", &validity),
            ],
        )?;
        Ok(())
    }

    fn store_incoming_sep(&self, modified: ServerTimestamp, s: SeparatorRecord) -> Result<()> {
        self.db.execute_named_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                               dateAdded)
             VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                    :dateAdded)",
            &[
                (":guid", &s.record_id.as_guid().as_ref()),
                (
                    ":parentGuid",
                    &s.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &(modified.as_millis() as i64)),
                (":kind", &SyncedBookmarkKind::Separator),
                (":dateAdded", &s.date_added),
            ],
        )?;
        Ok(())
    }

    fn maybe_store_href(&self, href: Option<&String>) -> Result<Url> {
        if let Some(href) = href {
            self.maybe_store_url(Some(Url::parse(href)?))
        } else {
            self.maybe_store_url(None)
        }
    }

    fn maybe_store_url(&self, url: Option<Url>) -> Result<Url> {
        if let Some(url) = url {
            if url.as_str().len() > URL_LENGTH_MAX {
                return Err(ErrorKind::InvalidPlaceInfo(InvalidPlaceInfo::UrlTooLong).into());
            }
            self.db.execute_named_cached(
                "INSERT OR IGNORE INTO moz_places(guid, url, url_hash, frecency)
                 VALUES(IFNULL((SELECT guid FROM moz_places
                                WHERE url_hash = hash(:url) AND
                                      url = :url),
                        generate_guid()), :url, hash(:url),
                        (CASE substr(:url, 1, 6) WHEN 'place:' THEN 0 ELSE -1 END))",
                &[(":url", &url.as_str())],
            )?;
            Ok(url)
        } else {
            Err(ErrorKind::InvalidPlaceInfo(InvalidPlaceInfo::NoUrl).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, PlacesApi, SyncConn};
    use crate::storage::bookmarks::BookmarkRootGuid;

    use crate::bookmark_sync::tests::SyncedBookmarkItem;
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use sync15::Payload;

    fn apply_incoming(api: &PlacesApi, records_json: Value) -> SyncConn<'_> {
        let conn = api.open_sync_connection().expect("should get a connection");

        let server_timestamp = ServerTimestamp(0.0);
        let applicator = IncomingApplicator::new(&conn);

        match records_json {
            Value::Array(records) => {
                for record in records {
                    let payload = Payload::from_json(record).unwrap();
                    applicator
                        .apply_payload(payload, server_timestamp)
                        .expect("Should apply incoming and stage outgoing records");
                }
            }
            Value::Object(_) => {
                let payload = Payload::from_json(records_json).unwrap();
                applicator
                    .apply_payload(payload, server_timestamp)
                    .expect("Should apply incoming and stage outgoing records");
            }
            _ => panic!("unexpected json value"),
        }

        conn
    }

    fn assert_incoming_creates_mirror_item(record_json: Value, expected: &SyncedBookmarkItem) {
        let guid = record_json["id"]
            .as_str()
            .expect("id must be a string")
            .to_string();
        let api = new_mem_api();
        let conn = apply_incoming(&api, record_json);
        let got = SyncedBookmarkItem::get(&conn, &guid.into())
            .expect("should work")
            .expect("item should exist");
        assert_eq!(*expected, got);
    }

    #[test]
    fn test_apply_bookmark() {
        assert_incoming_creates_mirror_item(
            json!({
                "id": "bookmarkAAAA",
                "type": "bookmark",
                "parentid": "unfiled",
                "parentName": "unfiled",
                "dateAdded": 1_381_542_355_843u64,
                "title": "A",
                "bmkUri": "http://example.com/a",
                "tags": ["foo", "bar"],
                "keyword": "baz",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Bookmark)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .title(Some("A"))
                .url(Some("http://example.com/a"))
                .tags(vec!["foo".into(), "bar".into()])
                .keyword(Some("baz")),
        );
    }

    #[test]
    fn test_apply_folder() {
        let children = (1..sql_support::default_max_variable_number() * 2)
            .map(|i| SyncGuid(format!("{:A>12}", i)))
            .collect::<Vec<_>>();
        let value = serde_json::to_value(BookmarkItemRecord::from(FolderRecord {
            record_id: BookmarkRecordId::from_payload_id("folderAAAAAA".into()),
            parent_record_id: Some(BookmarkRecordId::from_payload_id("unfiled".into())),
            parent_title: Some("unfiled".into()),
            date_added: Some(0),
            has_dupe: true,
            title: Some("A".into()),
            children: children
                .iter()
                .map(|guid| BookmarkRecordId::from(guid.clone()))
                .collect(),
        }))
        .expect("Should serialize folder with children");
        assert_incoming_creates_mirror_item(
            value,
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Folder)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .title(Some("A"))
                .children(children),
        );
    }

    #[test]
    fn test_apply_tombstone() {
        assert_incoming_creates_mirror_item(
            json!({
                "id": "deadbeef____",
                "deleted": true
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .deleted(true),
        );
    }

    #[test]
    fn test_apply_query() {
        // First check that various inputs result in the expected records in
        // the mirror table.

        // A valid query (which actually looks just like a bookmark, but that's ok)
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1_381_542_355_843u64,
                "title": "Some query",
                "bmkUri": "place:tag=foo",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Query)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .title(Some("Some query"))
                .url(Some("place:tag=foo")),
        );

        // A query with an old "type=" param and a valid folderName. Should
        // get Reupload due to rewriting the URL.
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "bmkUri": "place:type=7",
                "folderName": "a-folder-name",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Reupload)
                .kind(SyncedBookmarkKind::Query)
                .url(Some("place:tag=a-folder-name")),
        );

        // A query with an old "type=" param and an invalid folderName. Should
        // get replaced with an empty URL
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "bmkUri": "place:type=7",
                "folderName": "",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Replace)
                .kind(SyncedBookmarkKind::Query)
                .url(None),
        );

        // A query with an old "folder=" but no excludeItems - should be
        // marked as Reupload due to the URL being rewritten.
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "bmkUri": "place:folder=123",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Reupload)
                .kind(SyncedBookmarkKind::Query)
                .url(Some("place:folder=123&excludeItems=1")),
        );

        // A query with an old "folder=" and already with  excludeItems -
        // should be marked as Valid
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "bmkUri": "place:folder=123&excludeItems=1",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Query)
                .url(Some("place:folder=123&excludeItems=1")),
        );

        // A query with a URL that can't be parsed.
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "bmkUri": "foo",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Replace)
                .kind(SyncedBookmarkKind::Query)
                .url(None),
        );

        // With a missing URL
        assert_incoming_creates_mirror_item(
            json!({
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Replace)
                .kind(SyncedBookmarkKind::Query)
                .url(None),
        );
    }

    #[test]
    fn test_apply_sep() {
        // Separators don't have much variation.
        assert_incoming_creates_mirror_item(
            json!({
                "id": "sep1________",
                "type": "separator",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Separator)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .needs_merge(true),
        );
    }

    #[test]
    fn test_apply_livemark() {
        // A livemark with missing URLs
        assert_incoming_creates_mirror_item(
            json!({
                "id": "livemark1___",
                "type": "livemark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Replace)
                .kind(SyncedBookmarkKind::Livemark)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .needs_merge(true)
                .feed_url(None)
                .site_url(None),
        );
        // Valid feed_url but invalid site_url is considered "valid", but the
        // invalid URL is dropped.
        assert_incoming_creates_mirror_item(
            json!({
                "id": "livemark1___",
                "type": "livemark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "feedUri": "http://example.com",
                "siteUri": "foo"
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Livemark)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .needs_merge(true)
                .feed_url(Some("http://example.com/"))
                .site_url(None),
        );
        // Everything valid
        assert_incoming_creates_mirror_item(
            json!({
                "id": "livemark1___",
                "type": "livemark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "feedUri": "http://example.com",
                "siteUri": "http://example.com/something"
            }),
            &SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Livemark)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .needs_merge(true)
                .feed_url(Some("http://example.com/"))
                .site_url(Some("http://example.com/something")),
        );
    }
}
