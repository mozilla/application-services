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
use crate::types::serialize_unknown_fields;
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use sql_support::{self, ConnExt};
use std::{collections::HashSet, iter};
use sync15::bso::{IncomingBso, IncomingKind};
use sync15::ServerTimestamp;
use sync_guid::Guid as SyncGuid;
use url::Url;

// From Desktop's Ci.nsINavHistoryQueryOptions, but we define it as a str
// as that's how we use it here.
const RESULTS_AS_TAG_CONTENTS: &str = "7";

/// Manages the application of incoming records into the moz_bookmarks_synced
/// and related tables.
pub struct IncomingApplicator<'a> {
    db: &'a Connection,
    // For tests to override chunk sizes so they can finish quicker!
    default_max_variable_number: Option<usize>,
}

impl<'a> IncomingApplicator<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self {
            db,
            default_max_variable_number: None,
        }
    }

    pub fn apply_bso(&self, record: IncomingBso) -> Result<()> {
        let timestamp = record.envelope.modified;
        let mut validity = SyncedBookmarkValidity::Valid;
        let json_content = record.into_content_with_fixup::<BookmarkItemRecord>(|json| {
            validity = fixup_bookmark_json(json)
        });
        match json_content.kind {
            IncomingKind::Tombstone => {
                self.store_incoming_tombstone(
                    timestamp,
                    BookmarkRecordId::from_payload_id(json_content.envelope.id.clone()).as_guid(),
                )?;
            }
            IncomingKind::Content(item) => match item {
                BookmarkItemRecord::Bookmark(b) => {
                    self.store_incoming_bookmark(timestamp, &b, validity)?
                }
                BookmarkItemRecord::Query(q) => {
                    self.store_incoming_query(timestamp, &q, validity)?
                }
                BookmarkItemRecord::Folder(f) => {
                    self.store_incoming_folder(timestamp, &f, validity)?
                }
                BookmarkItemRecord::Livemark(l) => {
                    self.store_incoming_livemark(timestamp, &l, validity)?
                }
                BookmarkItemRecord::Separator(s) => {
                    self.store_incoming_sep(timestamp, &s, validity)?
                }
            },
            IncomingKind::Malformed => {
                trace!(
                    "skipping malformed bookmark record: {}",
                    json_content.envelope.id
                );
                error_support::report_error!(
                    "malformed-incoming-bookmark",
                    "Malformed bookmark record"
                );
            }
        }
        Ok(())
    }

    fn store_incoming_bookmark(
        &self,
        modified: ServerTimestamp,
        b: &BookmarkRecord,
        mut validity: SyncedBookmarkValidity,
    ) -> Result<()> {
        let url = match self.maybe_store_href(b.url.as_deref()) {
            Ok(u) => Some(String::from(u)),
            Err(e) => {
                warn!("Incoming bookmark has an invalid URL: {:?}", e);
                // The bookmark has an invalid URL, so we can't apply it.
                set_replace(&mut validity);
                None
            }
        };

        self.db.execute_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, keyword, validity, unknownFields, placeId)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, NULLIF(:title, ""), :keyword, :validity, :unknownFields,
                      CASE WHEN :url ISNULL
                      THEN NULL
                      ELSE (SELECT id FROM moz_places
                            WHERE url_hash = hash(:url) AND
                            url = :url)
                      END
                      )"#,
            &[
                (
                    ":guid",
                    &b.record_id.as_guid().as_str() as &dyn rusqlite::ToSql,
                ),
                (
                    ":parentGuid",
                    &b.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &modified.as_millis()),
                (":kind", &SyncedBookmarkKind::Bookmark),
                (":dateAdded", &b.date_added),
                (":title", &maybe_truncate_title(&b.title.as_deref())),
                (":keyword", &b.keyword),
                (":validity", &validity),
                (":url", &url),
                (":unknownFields", &serialize_unknown_fields(&b.unknown_fields)?),
            ],
        )?;
        for t in b.tags.iter() {
            self.db.execute_cached(
                "INSERT OR IGNORE INTO moz_tags(tag, lastModified)
                 VALUES(:tag, now())",
                &[(":tag", &t)],
            )?;
            self.db.execute_cached(
                "INSERT INTO moz_bookmarks_synced_tag_relation(itemId, tagId)
                 VALUES((SELECT id FROM moz_bookmarks_synced
                         WHERE guid = :guid),
                        (SELECT id FROM moz_tags
                         WHERE tag = :tag))",
                &[(":guid", b.record_id.as_guid().as_str()), (":tag", t)],
            )?;
        }
        Ok(())
    }

    fn store_incoming_folder(
        &self,
        modified: ServerTimestamp,
        f: &FolderRecord,
        validity: SyncedBookmarkValidity,
    ) -> Result<()> {
        self.db.execute_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, validity, unknownFields, title)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, :validity, :unknownFields, NULLIF(:title, ""))"#,
            &[
                (
                    ":guid",
                    &f.record_id.as_guid().as_str() as &dyn rusqlite::ToSql,
                ),
                (
                    ":parentGuid",
                    &f.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &modified.as_millis()),
                (":kind", &SyncedBookmarkKind::Folder),
                (":dateAdded", &f.date_added),
                (":title", &maybe_truncate_title(&f.title.as_deref())),
                (":validity", &validity),
                (
                    ":unknownFields",
                    &serialize_unknown_fields(&f.unknown_fields)?,
                ),
            ],
        )?;
        let default_max_variable_number = self
            .default_max_variable_number
            .unwrap_or_else(sql_support::default_max_variable_number);
        sql_support::each_sized_chunk(
            &f.children,
            // -1 because we want to leave an extra binding parameter (`?1`)
            // for the folder's GUID.
            default_max_variable_number - 1,
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
                    rusqlite::params_from_iter(
                        iter::once(&f.record_id)
                            .chain(chunk.iter())
                            .map(|id| id.as_guid().as_str()),
                    ),
                )?;
                Ok(())
            },
        )?;
        Ok(())
    }

    fn store_incoming_tombstone(&self, modified: ServerTimestamp, guid: &SyncGuid) -> Result<()> {
        self.db.execute_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge,
                                               dateAdded, isDeleted)
             VALUES(:guid, NULL, :serverModified, 1, 0, 1)",
            &[
                (":guid", guid as &dyn rusqlite::ToSql),
                (":serverModified", &modified.as_millis()),
            ],
        )?;
        Ok(())
    }

    fn maybe_rewrite_and_store_query_url(
        &self,
        tag_folder_name: Option<&str>,
        record_id: &BookmarkRecordId,
        url: Url,
        validity: &mut SyncedBookmarkValidity,
    ) -> Result<Option<Url>> {
        // wow - this  is complex, but markh is struggling to see how to
        // improve it
        let maybe_url = {
            // If the URL has `type={RESULTS_AS_TAG_CONTENTS}` then we
            // rewrite the URL as `place:tag=...`
            // Sadly we can't use `url.query_pairs()` here as the format of
            // the url is, eg, `place:type=7` - ie, the "params" are actually
            // the path portion of the URL.
            let parse = url::form_urlencoded::parse(url.path().as_bytes());
            if parse
                .clone()
                .any(|(k, v)| k == "type" && v == RESULTS_AS_TAG_CONTENTS)
            {
                if let Some(t) = tag_folder_name {
                    validate_tag(t)
                        .ensure_valid()
                        .and_then(|tag| Ok(Url::parse(&format!("place:tag={}", tag))?))
                        .map(|url| {
                            set_reupload(validity);
                            Some(url)
                        })
                        .unwrap_or_else(|_| {
                            set_replace(validity);
                            None
                        })
                } else {
                    set_replace(validity);
                    None
                }
            } else {
                // If we have `folder=...` the folder value is a row_id
                // from desktop, so useless to us - so we append `&excludeItems=1`
                // if it isn't already there.
                if parse.clone().any(|(k, _)| k == "folder") {
                    if parse.clone().any(|(k, v)| k == "excludeItems" && v == "1") {
                        Some(url)
                    } else {
                        // need to add excludeItems, and I guess we should do
                        // it properly without resorting to string manipulation...
                        let tail = url::form_urlencoded::Serializer::new(String::new())
                            .extend_pairs(parse)
                            .append_pair("excludeItems", "1")
                            .finish();
                        set_reupload(validity);
                        Some(Url::parse(&format!("place:{}", tail))?)
                    }
                } else {
                    // it appears to be fine!
                    Some(url)
                }
            }
        };
        Ok(match self.maybe_store_url(maybe_url) {
            Ok(url) => Some(url),
            Err(e) => {
                warn!("query {} has invalid URL: {:?}", record_id.as_guid(), e);
                set_replace(validity);
                None
            }
        })
    }

    fn store_incoming_query(
        &self,
        modified: ServerTimestamp,
        q: &QueryRecord,
        mut validity: SyncedBookmarkValidity,
    ) -> Result<()> {
        let url = match q.url.as_ref().and_then(|href| Url::parse(href).ok()) {
            Some(url) => self.maybe_rewrite_and_store_query_url(
                q.tag_folder_name.as_deref(),
                &q.record_id,
                url,
                &mut validity,
            )?,
            None => {
                warn!("query {} has invalid URL", &q.record_id.as_guid(),);
                set_replace(&mut validity);
                None
            }
        };

        self.db.execute_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, validity, unknownFields, placeId)
               VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                      :dateAdded, NULLIF(:title, ""), :validity, :unknownFields,
                      (SELECT id FROM moz_places
                            WHERE url_hash = hash(:url) AND
                            url = :url
                      )
                     )"#,
            &[
                (
                    ":guid",
                    &q.record_id.as_guid().as_str() as &dyn rusqlite::ToSql,
                ),
                (
                    ":parentGuid",
                    &q.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &modified.as_millis()),
                (":kind", &SyncedBookmarkKind::Query),
                (":dateAdded", &q.date_added),
                (":title", &maybe_truncate_title(&q.title.as_deref())),
                (":validity", &validity),
                (
                    ":unknownFields",
                    &serialize_unknown_fields(&q.unknown_fields)?,
                ),
                (":url", &url.map(String::from)),
            ],
        )?;
        Ok(())
    }

    fn store_incoming_livemark(
        &self,
        modified: ServerTimestamp,
        l: &LivemarkRecord,
        mut validity: SyncedBookmarkValidity,
    ) -> Result<()> {
        // livemarks don't store a reference to the place, so we validate it manually.
        fn validate_href(h: &Option<String>, guid: &SyncGuid, what: &str) -> Option<String> {
            match h {
                Some(h) => match Url::parse(h) {
                    Ok(url) => {
                        let s = url.to_string();
                        if s.len() > URL_LENGTH_MAX {
                            warn!("Livemark {} has a {} URL which is too long", &guid, what);
                            None
                        } else {
                            Some(s)
                        }
                    }
                    Err(e) => {
                        warn!("Livemark {} has an invalid {} URL: {:?}", &guid, what, e);
                        None
                    }
                },
                None => {
                    warn!("Livemark {} has no {} URL", &guid, what);
                    None
                }
            }
        }
        let feed_url = validate_href(&l.feed_url, l.record_id.as_guid(), "feed");
        let site_url = validate_href(&l.site_url, l.record_id.as_guid(), "site");

        if feed_url.is_none() {
            set_replace(&mut validity);
        }

        self.db.execute_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                               dateAdded, title, feedURL, siteURL, validity)
             VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                    :dateAdded, :title, :feedUrl, :siteUrl, :validity)",
            &[
                (
                    ":guid",
                    &l.record_id.as_guid().as_str() as &dyn rusqlite::ToSql,
                ),
                (
                    ":parentGuid",
                    &l.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &modified.as_millis()),
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

    fn store_incoming_sep(
        &self,
        modified: ServerTimestamp,
        s: &SeparatorRecord,
        validity: SyncedBookmarkValidity,
    ) -> Result<()> {
        self.db.execute_cached(
            "REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                               dateAdded, validity, unknownFields)
             VALUES(:guid, :parentGuid, :serverModified, 1, :kind,
                    :dateAdded, :validity, :unknownFields)",
            &[
                (
                    ":guid",
                    &s.record_id.as_guid().as_str() as &dyn rusqlite::ToSql,
                ),
                (
                    ":parentGuid",
                    &s.parent_record_id.as_ref().map(BookmarkRecordId::as_guid),
                ),
                (":serverModified", &modified.as_millis()),
                (":kind", &SyncedBookmarkKind::Separator),
                (":dateAdded", &s.date_added),
                (":validity", &validity),
                (
                    ":unknownFields",
                    &serialize_unknown_fields(&s.unknown_fields)?,
                ),
            ],
        )?;
        Ok(())
    }

    fn maybe_store_href(&self, href: Option<&str>) -> Result<Url> {
        if let Some(href) = href {
            self.maybe_store_url(Some(Url::parse(href)?))
        } else {
            self.maybe_store_url(None)
        }
    }

    fn maybe_store_url(&self, url: Option<Url>) -> Result<Url> {
        if let Some(url) = url {
            if url.as_str().len() > URL_LENGTH_MAX {
                return Err(Error::InvalidPlaceInfo(InvalidPlaceInfo::UrlTooLong));
            }
            self.db.execute_cached(
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
            Err(Error::InvalidPlaceInfo(InvalidPlaceInfo::NoUrl))
        }
    }
}

/// Go through the raw JSON value and try to fixup invalid data -- this mostly means fields with
/// invalid types.
///
/// This is extra important since bookmarks form a tree.  If a parent node is invalid, then we will
/// have issues trying to merge its children.
fn fixup_bookmark_json(json: &mut JsonValue) -> SyncedBookmarkValidity {
    let mut validity = SyncedBookmarkValidity::Valid;
    // the json value should always be on object, if not don't try to do any fixups.  The result will
    // be that into_content_with_fixup() returns an IncomingContent with IncomingKind::Malformed.
    if let JsonValue::Object(obj) = json {
        obj.entry("parentid")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("title")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("bmkUri")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("folderName")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("feedUri")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("siteUri")
            .and_modify(|v| fixup_optional_str(v, &mut validity));
        obj.entry("dateAdded")
            .and_modify(|v| fixup_optional_i64(v, &mut validity));
        obj.entry("keyword")
            .and_modify(|v| fixup_optional_keyword(v, &mut validity));
        obj.entry("tags")
            .and_modify(|v| fixup_optional_tags(v, &mut validity));
    }
    validity
}

fn fixup_optional_str(val: &mut JsonValue, validity: &mut SyncedBookmarkValidity) {
    if !matches!(val, JsonValue::String(_) | JsonValue::Null) {
        set_reupload(validity);
        *val = JsonValue::Null;
    }
}

fn fixup_optional_i64(val: &mut JsonValue, validity: &mut SyncedBookmarkValidity) {
    match val {
        // There's basically nothing to do for numbers, although we could try to drop any fraction.
        JsonValue::Number(_) => (),
        JsonValue::String(s) => {
            set_reupload(validity);
            if let Ok(n) = s.parse::<u64>() {
                *val = JsonValue::Number(n.into());
            } else {
                *val = JsonValue::Null;
            }
        }
        JsonValue::Null => (),
        _ => {
            set_reupload(validity);
            *val = JsonValue::Null;
        }
    }
}

// Fixup incoming keywords by lowercasing them and removing surrounding whitespace
//
// Like Desktop, we don't reupload if a keyword needs to be fixed-up
// trailing whitespace, or isn't lowercase.
fn fixup_optional_keyword(val: &mut JsonValue, validity: &mut SyncedBookmarkValidity) {
    match val {
        JsonValue::String(s) => {
            let fixed = s.trim().to_lowercase();
            if fixed.is_empty() {
                *val = JsonValue::Null;
            } else if fixed != *s {
                *val = JsonValue::String(fixed);
            }
        }
        JsonValue::Null => (),
        _ => {
            set_reupload(validity);
            *val = JsonValue::Null;
        }
    }
}

fn fixup_optional_tags(val: &mut JsonValue, validity: &mut SyncedBookmarkValidity) {
    match val {
        JsonValue::Array(tags) => {
            let mut valid_tags = HashSet::with_capacity(tags.len());
            for v in tags {
                if let JsonValue::String(tag) = v {
                    let tag = match validate_tag(tag) {
                        ValidatedTag::Invalid(t) => {
                            trace!("Incoming bookmark has invalid tag: {:?}", t);
                            set_reupload(validity);
                            continue;
                        }
                        ValidatedTag::Normalized(t) => {
                            set_reupload(validity);
                            t
                        }
                        ValidatedTag::Original(t) => t,
                    };
                    if !valid_tags.insert(tag) {
                        trace!("Incoming bookmark has duplicate tag: {:?}", tag);
                        set_reupload(validity);
                    }
                } else {
                    trace!("Incoming bookmark has unexpected tag: {:?}", v);
                    set_reupload(validity);
                }
            }
            *val = JsonValue::Array(valid_tags.into_iter().map(JsonValue::from).collect());
        }
        JsonValue::Null => (),
        _ => {
            set_reupload(validity);
            *val = JsonValue::Null;
        }
    }
}

fn set_replace(validity: &mut SyncedBookmarkValidity) {
    if *validity < SyncedBookmarkValidity::Replace {
        *validity = SyncedBookmarkValidity::Replace;
    }
}

fn set_reupload(validity: &mut SyncedBookmarkValidity) {
    if *validity < SyncedBookmarkValidity::Reupload {
        *validity = SyncedBookmarkValidity::Reupload;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, PlacesApi};
    use crate::bookmark_sync::record::{BookmarkItemRecord, FolderRecord};
    use crate::bookmark_sync::tests::SyncedBookmarkItem;
    use crate::storage::bookmarks::BookmarkRootGuid;
    use crate::types::UnknownFields;
    use serde_json::{json, Value};

    fn apply_incoming(api: &PlacesApi, records_json: Value) {
        let db = api.get_sync_connection().expect("should get a db mutex");
        let conn = db.lock();

        let mut applicator = IncomingApplicator::new(&conn);
        applicator.default_max_variable_number = Some(5);

        match records_json {
            Value::Array(records) => {
                for record in records {
                    applicator
                        .apply_bso(IncomingBso::from_test_content(record))
                        .expect("Should apply incoming and stage outgoing records");
                }
            }
            Value::Object(_) => {
                applicator
                    .apply_bso(IncomingBso::from_test_content(records_json))
                    .expect("Should apply incoming and stage outgoing records");
            }
            _ => panic!("unexpected json value"),
        }
    }

    fn assert_incoming_creates_mirror_item(record_json: Value, expected: &SyncedBookmarkItem) {
        let guid = record_json["id"]
            .as_str()
            .expect("id must be a string")
            .to_string();
        let api = new_mem_api();
        apply_incoming(&api, record_json);
        let got = SyncedBookmarkItem::get(&api.get_sync_connection().unwrap().lock(), &guid.into())
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
            SyncedBookmarkItem::new()
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
        // apply_incoming arranges for the chunk-size to be 5, so to ensure
        // we exercise the chunking done for folders we only need more than that.
        let children = (1..6)
            .map(|i| SyncGuid::from(format!("{:A>12}", i)))
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
            unknown_fields: UnknownFields::new(),
        }))
        .expect("Should serialize folder with children");
        assert_incoming_creates_mirror_item(
            value,
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
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
            SyncedBookmarkItem::new()
                .validity(SyncedBookmarkValidity::Valid)
                .kind(SyncedBookmarkKind::Livemark)
                .parent_guid(Some(&BookmarkRootGuid::Unfiled.as_guid()))
                .needs_merge(true)
                .feed_url(Some("http://example.com/"))
                .site_url(Some("http://example.com/something")),
        );
    }

    #[test]
    fn test_apply_unknown() {
        let api = new_mem_api();
        let db = api.get_sync_connection().expect("should get a db mutex");
        let conn = db.lock();
        let applicator = IncomingApplicator::new(&conn);

        let record = json!({
            "id": "unknownAAAA",
            "type": "fancy",
        });
        let inc = IncomingBso::from_test_content(record);
        // We report an error for an invalid type but don't fail.
        applicator
            .apply_bso(inc)
            .expect("Should not fail with a record with unknown type");
    }
}
