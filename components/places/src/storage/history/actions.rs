/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Structs to handle actions that mutate the History DB
//!
//! Places DB operations are complex and often involve several layers of changes which makes them
//! hard to test.  For example, a function that deletes visits in a date range needs to:
//!   - Calculate which visits to delete
//!   - Delete the visits
//!   - Insert visit tombstones
//!   - Update the frecency for non-orphaned affected pages
//!   - Delete orphaned pages
//!   - Insert page tombstones for deleted synced pages
//!
//! Test all of this functionality at once leads to ugly tests that are hard to reason about and
//! hard to change.  This is especially true since many steps have multiple branches which
//! multiplies the complexity.
//!
//! This module is intended to split up operations to make testing simpler.  It defines an enum
//! whose variants encapsulate particular actions.  We can use that enum to split operations into
//! multiple parts, each which can be tested separately: code that calculates which actions to run
//! and the code to run each action.
//!
//! Right now, only a couple function use this system, but hopefully we can use it more in the
//! future.

use super::{cleanup_pages, PageToClean};
use crate::error::Result;
use crate::{PlacesDb, RowId};
use rusqlite::Row;
use sql_support::ConnExt;
use std::collections::HashSet;

/// Enum whose variants describe a particular action on the DB
#[derive(Debug, PartialEq, Eq)]
pub(super) enum DbAction {
    /// Delete visit rows from the DB.
    DeleteVisitRows { visit_ids: HashSet<RowId> },
    /// Recalculate the moz_places data, including frecency, after changes to their visits.  This
    /// also deletes orphaned pages (pages whose visits have all been deleted).
    RecalcPages { page_ids: HashSet<RowId> },
    /// Delete rows in pending temp tables.  This should be done after any changes to the
    /// moz_places table.
    ///
    /// Deleting from these tables triggers changes to the `moz_origins` table. See
    /// `sql/create_shared_temp_tables.sql` and `sql/create_shared_triggers.sql` for details.
    DeleteFromPendingTempTables,
}

impl DbAction {
    pub(super) fn apply(self, db: &PlacesDb) -> Result<()> {
        match self {
            Self::DeleteVisitRows { visit_ids } => Self::delete_visit_rows(db, visit_ids),
            Self::RecalcPages { page_ids } => Self::recalc_pages(db, page_ids),
            Self::DeleteFromPendingTempTables => Self::delete_from_pending_temp_tables(db),
        }
    }

    pub(super) fn apply_all(db: &PlacesDb, actions: Vec<Self>) -> Result<()> {
        for action in actions {
            action.apply(db)?;
        }
        Ok(())
    }

    fn delete_visit_rows(db: &PlacesDb, visit_ids: HashSet<RowId>) -> Result<()> {
        sql_support::each_chunk(&Vec::from_iter(visit_ids), |chunk, _| -> Result<()> {
            let var_repeat = sql_support::repeat_sql_vars(chunk.len());
            let params = rusqlite::params_from_iter(chunk);
            db.execute_cached(
                &format!(
                    "
                    INSERT OR IGNORE INTO moz_historyvisit_tombstones(place_id, visit_date)
                    SELECT place_id, visit_date
                    FROM moz_historyvisits
                    WHERE id IN ({})
                    ",
                    var_repeat,
                ),
                params.clone(),
            )?;

            db.execute_cached(
                &format!("DELETE FROM moz_historyvisits WHERE id IN ({})", var_repeat),
                params,
            )?;
            Ok(())
        })?;
        Ok(())
    }

    fn recalc_pages(db: &PlacesDb, page_ids: HashSet<RowId>) -> Result<()> {
        let mut pages_to_clean: Vec<PageToClean> = vec![];
        sql_support::each_chunk(&Vec::from_iter(page_ids), |chunk, _| -> Result<()> {
            pages_to_clean.append(&mut db.query_rows_and_then_cached(
                &format!(
                    "SELECT
                    id,
                    (foreign_count != 0) AS has_foreign,
                    ((last_visit_date_local + last_visit_date_remote) != 0) as has_visits,
                    sync_status
                FROM moz_places
                WHERE id IN ({})",
                    sql_support::repeat_sql_vars(chunk.len())
                ),
                rusqlite::params_from_iter(chunk),
                PageToClean::from_row,
            )?);
            Ok(())
        })?;
        cleanup_pages(db, &pages_to_clean)?;
        Ok(())
    }

    fn delete_from_pending_temp_tables(db: &PlacesDb) -> Result<()> {
        crate::storage::delete_pending_temp_tables(db)
    }
}

/// Stores a visit that we want to delete
///
/// We build a Vec of these from queries against the `moz_historyvisits` table, then transform that
/// into a `Vec<DbAction>`.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct VisitToDelete {
    pub(super) visit_id: RowId,
    pub(super) page_id: RowId,
}

impl VisitToDelete {
    /// Create a VisitToDelete from a query row
    ///
    /// The query must that includes the `id` and `place_id` columns from `moz_historyvisits`.
    pub(super) fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            visit_id: row.get("id")?,
            page_id: row.get("place_id")?,
        })
    }
}

/// Create a Vec<DbAction> from a Vec<VisitToDelete>
pub(super) fn db_actions_from_visits_to_delete(
    visits_to_delete: Vec<VisitToDelete>,
) -> Vec<DbAction> {
    let mut visit_ids = HashSet::<RowId>::new();
    let mut page_ids = HashSet::<RowId>::new();
    for visit_to_delete in visits_to_delete.into_iter() {
        visit_ids.insert(visit_to_delete.visit_id);
        page_ids.insert(visit_to_delete.page_id);
    }
    vec![
        DbAction::DeleteVisitRows { visit_ids },
        DbAction::RecalcPages { page_ids },
        DbAction::DeleteFromPendingTempTables,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::VisitObservation;
    use crate::storage::bookmarks::*;
    use crate::storage::history::apply_observation;
    use crate::types::VisitType;
    use crate::{frecency, ConnectionType, SyncStatus};
    use rusqlite::params;
    use rusqlite::types::{FromSql, ToSql};
    use std::time::Duration;
    use sync_guid::Guid;
    use types::Timestamp;
    use url::Url;

    fn query_vec<T: FromSql>(conn: &PlacesDb, sql: &str, params: &[&dyn ToSql]) -> Vec<T> {
        conn.prepare(sql)
            .unwrap()
            .query_map(params, |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<T>>>()
            .unwrap()
    }

    fn query_vec_pairs<T: FromSql, V: FromSql>(
        conn: &PlacesDb,
        sql: &str,
        params: &[&dyn ToSql],
    ) -> Vec<(T, V)> {
        conn.prepare(sql)
            .unwrap()
            .query_map(params, |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<(T, V)>>>()
            .unwrap()
    }

    fn query_visit_ids(conn: &PlacesDb) -> Vec<RowId> {
        query_vec(conn, "SELECT id FROM moz_historyvisits ORDER BY id", &[])
    }

    fn query_visit_tombstones(conn: &PlacesDb) -> Vec<(RowId, Timestamp)> {
        query_vec_pairs(
            conn,
            "
            SELECT place_id, visit_date
            FROM moz_historyvisit_tombstones
            ORDER BY place_id, visit_date
            ",
            &[],
        )
    }

    fn query_page_ids(conn: &PlacesDb) -> Vec<RowId> {
        query_vec(conn, "SELECT id FROM moz_places ORDER BY id", &[])
    }

    fn query_page_tombstones(conn: &PlacesDb) -> Vec<Guid> {
        query_vec(
            conn,
            "SELECT guid FROM moz_places_tombstones ORDER BY guid",
            &[],
        )
    }

    struct TestPage {
        id: RowId,
        guid: Guid,
        url: Url,
        visit_ids: Vec<RowId>,
        visit_dates: Vec<Timestamp>,
    }

    impl TestPage {
        fn new(conn: &mut PlacesDb, url: &str, visit_dates: &[Timestamp]) -> Self {
            let url = Url::parse(url).unwrap();
            let mut visit_ids = vec![];

            for date in visit_dates {
                visit_ids.push(
                    apply_observation(
                        conn,
                        VisitObservation::new(url.clone())
                            .with_visit_type(VisitType::Link)
                            .with_at(*date),
                    )
                    .unwrap()
                    .unwrap(),
                );
            }

            let (id, guid) = conn
                .query_row(
                    "
                SELECT p.id, p.guid
                FROM moz_places p
                JOIN moz_historyvisits v ON p.id = v.place_id
                WHERE v.id = ?",
                    [visit_ids[0]],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();

            Self {
                id,
                guid,
                visit_ids,
                url,
                visit_dates: Vec::from_iter(visit_dates.iter().cloned()),
            }
        }

        fn set_sync_status(&self, conn: &PlacesDb, sync_status: SyncStatus) {
            conn.execute(
                "UPDATE moz_places SET sync_status = ? WHERE id = ?",
                params! {sync_status, self.id },
            )
            .unwrap();
        }

        fn query_frecency(&self, conn: &PlacesDb) -> i32 {
            conn.query_row(
                "SELECT frecency FROM moz_places WHERE id = ?",
                [self.id],
                |row| row.get::<usize, i32>(0),
            )
            .unwrap()
        }

        fn calculate_frecency(&self, conn: &PlacesDb) -> i32 {
            frecency::calculate_frecency(
                conn,
                &frecency::DEFAULT_FRECENCY_SETTINGS,
                self.id.0,
                None,
            )
            .unwrap()
        }

        fn bookmark(&self, conn: &PlacesDb, title: &str) {
            insert_bookmark(
                conn,
                InsertableBookmark {
                    parent_guid: BookmarkRootGuid::Unfiled.into(),
                    position: BookmarkPosition::Append,
                    date_added: None,
                    last_modified: None,
                    guid: None,
                    url: self.url.clone(),
                    title: Some(title.to_owned()),
                }
                .into(),
            )
            .unwrap();
        }
    }

    #[test]
    fn test_delete_visit_rows() {
        let mut conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).unwrap();
        let yesterday = Timestamp::now()
            .checked_sub(Duration::from_secs(60 * 60 * 24))
            .unwrap();
        let page = TestPage::new(
            &mut conn,
            "http://example.com/",
            &[
                Timestamp(yesterday.0 + 100),
                Timestamp(yesterday.0 + 200),
                Timestamp(yesterday.0 + 300),
            ],
        );

        DbAction::DeleteVisitRows {
            visit_ids: HashSet::from_iter([page.visit_ids[0], page.visit_ids[1]]),
        }
        .apply(&conn)
        .unwrap();

        assert_eq!(query_visit_ids(&conn), vec![page.visit_ids[2]]);
        assert_eq!(
            query_visit_tombstones(&conn),
            vec![
                (page.id, page.visit_dates[0]),
                (page.id, page.visit_dates[1]),
            ]
        );
    }

    #[test]
    fn test_recalc_pages() {
        let mut conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).unwrap();
        let yesterday = Timestamp::now()
            .checked_sub(Duration::from_secs(60 * 60 * 24))
            .unwrap();
        let page_with_visits_left = TestPage::new(
            &mut conn,
            "http://example.com/1",
            &[Timestamp(yesterday.0 + 100), Timestamp(yesterday.0 + 200)],
        );
        let page_with_no_visits_unsynced = TestPage::new(
            &mut conn,
            "http://example.com/2",
            &[Timestamp(yesterday.0 + 300)],
        );
        let page_with_no_visits_synced = TestPage::new(
            &mut conn,
            "http://example.com/2",
            &[Timestamp(yesterday.0 + 400)],
        );
        let page_with_no_visits_bookmarked = TestPage::new(
            &mut conn,
            "http://example.com/3",
            &[Timestamp(yesterday.0 + 500)],
        );

        page_with_no_visits_synced.set_sync_status(&conn, SyncStatus::Normal);
        page_with_no_visits_bookmarked.bookmark(&conn, "My Bookmark");

        DbAction::DeleteVisitRows {
            visit_ids: HashSet::from_iter([
                page_with_visits_left.visit_ids[0],
                page_with_no_visits_unsynced.visit_ids[0],
                page_with_no_visits_synced.visit_ids[0],
                page_with_no_visits_bookmarked.visit_ids[0],
            ]),
        }
        .apply(&conn)
        .unwrap();

        DbAction::RecalcPages {
            page_ids: HashSet::from_iter([
                page_with_visits_left.id,
                page_with_no_visits_unsynced.id,
                page_with_no_visits_synced.id,
                page_with_no_visits_bookmarked.id,
            ]),
        }
        .apply(&conn)
        .unwrap();

        assert_eq!(
            query_page_ids(&conn),
            [page_with_visits_left.id, page_with_no_visits_bookmarked.id]
        );
        assert_eq!(
            query_page_tombstones(&conn),
            [page_with_no_visits_synced.guid]
        );
        assert_eq!(
            page_with_visits_left.query_frecency(&conn),
            page_with_visits_left.calculate_frecency(&conn)
        );
        assert_eq!(
            page_with_no_visits_bookmarked.query_frecency(&conn),
            page_with_no_visits_bookmarked.calculate_frecency(&conn)
        );
    }

    #[test]
    fn test_delete_from_pending_temp_tables() {
        let mut conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).unwrap();
        let yesterday = Timestamp::now()
            .checked_sub(Duration::from_secs(60 * 60 * 24))
            .unwrap();
        let test_page = TestPage::new(
            &mut conn,
            "http://example.com/",
            &[
                Timestamp(yesterday.0 + 100),
                Timestamp(yesterday.0 + 200),
                Timestamp(yesterday.0 + 300),
            ],
        );
        DbAction::DeleteVisitRows {
            visit_ids: HashSet::from_iter([test_page.visit_ids[0]]),
        }
        .apply(&conn)
        .unwrap();
        DbAction::RecalcPages {
            page_ids: HashSet::from_iter([test_page.id]),
        }
        .apply(&conn)
        .unwrap();
        DbAction::DeleteFromPendingTempTables.apply(&conn).unwrap();
        assert_eq!(
            conn.query_one::<u32>("SELECT COUNT(*) FROM moz_updateoriginsinsert_temp")
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_one::<u32>("SELECT COUNT(*) FROM moz_updateoriginsupdate_temp")
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_one::<u32>("SELECT COUNT(*) FROM moz_updateoriginsdelete_temp")
                .unwrap(),
            0
        );
    }

    #[test]
    fn test_db_actions_from_visits_to_delete() {
        assert_eq!(
            db_actions_from_visits_to_delete(vec![
                VisitToDelete {
                    visit_id: RowId(1),
                    page_id: RowId(1),
                },
                VisitToDelete {
                    visit_id: RowId(2),
                    page_id: RowId(2),
                },
                VisitToDelete {
                    visit_id: RowId(3),
                    page_id: RowId(2),
                },
            ]),
            vec![
                DbAction::DeleteVisitRows {
                    visit_ids: HashSet::from_iter([RowId(1), RowId(2), RowId(3)])
                },
                DbAction::RecalcPages {
                    page_ids: HashSet::from_iter([RowId(1), RowId(2)])
                },
                DbAction::DeleteFromPendingTempTables,
            ],
        )
    }
}
