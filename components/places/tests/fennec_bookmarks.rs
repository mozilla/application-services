/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use places::{api::places_api::PlacesApi, ErrorKind, Result, Timestamp};
use rusqlite::types::{ToSql, ToSqlOutput};
use rusqlite::{Connection, NO_PARAMS};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use sync_guid::Guid;
use tempfile::tempdir;

fn empty_fennec_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("./fennec_bookmarks_schema.sql"))?;
    Ok(conn)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum FennecBookmarkType {
    Folder = 0,
    Bookmark = 1,
    Separator = 2,
}

impl ToSql for FennecBookmarkType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

#[derive(Clone, Debug)]
struct FennecBookmark {
    _id: i64,
    title: Option<String>,
    url: Option<String>,
    r#type: &'static FennecBookmarkType,
    parent: i64,
    position: i64,
    keyword: Option<String>,
    description: Option<String>,
    tags: Option<String>,
    favicon_id: Option<i64>,
    created: Option<Timestamp>,
    modified: Option<Timestamp>,
    guid: Guid,
    deleted: bool,
    local_version: i64,
    sync_version: i64,
}

impl FennecBookmark {
    fn insert_into_db(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(&
            "INSERT OR IGNORE INTO bookmarks(_id, title, url, type, parent, position, keyword,
                                             description, tags, favicon_id, created, modified,
                                             guid, deleted, localVersion, syncVersion)
             VALUES (:_id, :title, :url, :type, :parent, :position, :keyword, :description, :tags,
                     :favicon_id, :created, :modified, :guid, :deleted, :localVersion, :syncVersion)"
        )?;
        stmt.execute_named(rusqlite::named_params! {
            ":_id": self._id,
            ":title": self.title,
            ":url": self.url,
            ":type": self.r#type,
            ":parent": self.parent,
            ":position": self.position,
            ":keyword": self.keyword,
            ":description": self.description,
            ":tags": self.tags,
            ":favicon_id": self.favicon_id,
            ":created": self.created,
            ":modified": self.modified,
            ":guid": self.guid,
            ":deleted": self.deleted,
            ":localVersion": self.local_version,
            ":syncVersion": self.sync_version,
        })?;
        Ok(())
    }
}

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Helps debugging to use these instead of actually random ones.
fn next_guid() -> Guid {
    let c = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let v = format!("test{}_______", c);
    let s = &v[..12];
    Guid::from(s)
}

impl Default for FennecBookmark {
    fn default() -> Self {
        Self {
            _id: 0,
            title: None,
            url: None,
            r#type: &FennecBookmarkType::Bookmark,
            parent: 0,
            position: 0,
            keyword: None,
            description: None,
            tags: None,
            favicon_id: None,
            created: Some(Timestamp::now()),
            modified: Some(Timestamp::now()),
            guid: next_guid(),
            deleted: false,
            local_version: 1,
            sync_version: 0,
        }
    }
}

fn insert_bookmarks(conn: &Connection, bookmarks: &[FennecBookmark]) -> Result<()> {
    for b in bookmarks {
        b.insert_into_db(conn)?;
    }
    Ok(())
}

#[test]
fn test_import_unsupported_db_version() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let fennec_path = tmpdir.path().join("browser.db");
    let fennec_db = empty_fennec_db(&fennec_path)?;
    fennec_db.execute("PRAGMA user_version=99", NO_PARAMS)?;
    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;
    match places::import::import_fennec_bookmarks(&places_api, fennec_path)
        .unwrap_err()
        .kind()
    {
        ErrorKind::UnsupportedDatabaseVersion(_) => {}
        _ => unreachable!("Should fail with UnsupportedDatabaseVersion!"),
    }
    Ok(())
}

#[test]
fn test_import() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let fennec_path = tmpdir.path().join("browser.db");
    let fennec_db = empty_fennec_db(&fennec_path)?;

    let bookmarks = [
        // Roots.
        FennecBookmark {
            _id: 0,
            parent: 0, // The root node is its own parent.
            guid: Guid::from("places"),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: -3,
            parent: 0,
            position: 5,
            guid: Guid::from("pinned"),
            title: Some("Pinned".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 1,
            parent: 0,
            guid: Guid::from("mobile"),
            title: Some("Mobile Bookmarks".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 2,
            parent: 0,
            guid: Guid::from("toolbar"),
            title: Some("Bookmarks Toolbar".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 3,
            parent: 0,
            guid: Guid::from("menu"),
            title: Some("Bookmarks Menu".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 4,
            parent: 0,
            guid: Guid::from("tags"),
            title: Some("Tags".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 5,
            parent: 0,
            guid: Guid::from("unfiled"),
            title: Some("Other Bookmarks".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        // End of roots.
        FennecBookmark {
            _id: 6,
            parent: 1,
            title: Some("Firefox: About your browser".to_owned()),
            url: Some("about:firefox".to_owned()),
            position: 1,
            ..Default::default()
        },
        FennecBookmark {
            _id: 7,
            parent: 1,
            title: Some("Folder one".to_owned()),
            r#type: &FennecBookmarkType::Folder,
            ..Default::default()
        },
        FennecBookmark {
            _id: 8,
            parent: 7,
            title: Some("Foo".to_owned()),
            url: Some("https://bar.foo".to_owned()),
            position: -9_223_372_036_854_775_808, // Haaaaaa.
            favicon_id: Some(-2),                 // Hoooo.
            ..Default::default()
        },
        FennecBookmark {
            _id: 9,
            parent: 7,
            position: 0,
            r#type: &FennecBookmarkType::Separator,
            ..Default::default()
        },
        FennecBookmark {
            _id: 10,
            parent: 7,
            title: Some("Not a valid URL yo.".to_owned()),
            url: Some("foo bar unlimited edition".to_owned()),
            ..Default::default()
        },
        FennecBookmark {
            _id: 11,
            parent: -3,
            title: Some("Pinned Bookmark".to_owned()),
            url: Some("https://foo.bar".to_owned()),
            ..Default::default()
        },
    ];
    insert_bookmarks(&fennec_db, &bookmarks)?;

    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;

    let pinned = places::import::import_fennec_bookmarks(&places_api, fennec_path)?;
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].title, Some("Pinned Bookmark".to_owned()));

    // Uncomment the following to debug with cargo test -- --nocapture.
    // println!(
    //     "Places DB Path: {}",
    //     tmpdir.path().join("places.sqlite").to_str().unwrap()
    // );
    // ::std::process::exit(0);

    Ok(())
}
