/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{fetch_page_info, new_page_info, PageInfo, RowId};
use crate::db::PlacesDb;
use crate::error::Result;
use crate::frecency;
use crate::hash;
use crate::observation::VisitObservation;
use crate::types::{SyncGuid, SyncStatus, Timestamp, VisitTransition};
use rusqlite::types::ToSql;
use rusqlite::Result as RusqliteResult;
use rusqlite::{Connection, Row, NO_PARAMS};
use sql_support::{self, ConnExt};
use url::Url;

/// Returns the RowId of a new visit in moz_historyvisits, or None if no new visit was added.
pub fn apply_observation(db: &mut PlacesDb, visit_ob: VisitObservation) -> Result<Option<RowId>> {
    let tx = db.db.transaction()?;
    let result = apply_observation_direct(tx.conn(), visit_ob)?;
    tx.commit()?;
    Ok(result)
}

/// Returns the RowId of a new visit in moz_historyvisits, or None if no new visit was added.
pub fn apply_observation_direct(
    db: &Connection,
    visit_ob: VisitObservation,
) -> Result<Option<RowId>> {
    let mut page_info = match fetch_page_info(db, &visit_ob.url)? {
        Some(info) => info.page,
        None => new_page_info(db, &visit_ob.url, None)?,
    };
    let mut update_change_counter = false;
    let mut update_frec = false;
    let mut updates: Vec<(&str, &str, &ToSql)> = Vec::new();

    if let Some(ref title) = visit_ob.title {
        page_info.title = title.clone();
        updates.push(("title", ":title", &page_info.title));
        update_change_counter = true;
    }
    // There's a new visit, so update everything that implies. To help with
    // testing we return the rowid of the visit we added.
    let visit_row_id = match visit_ob.visit_type {
        Some(visit_type) => {
            // A single non-hidden visit makes the place non-hidden.
            if !visit_ob.get_is_hidden() {
                updates.push(("hidden", ":hidden", &false));
            }
            if visit_type == VisitTransition::Typed {
                page_info.typed += 1;
                updates.push(("typed", ":typed", &page_info.typed));
            }

            let at = visit_ob.at.unwrap_or_else(|| Timestamp::now());
            let is_remote = visit_ob.is_remote.unwrap_or(false);
            let row_id = add_visit(db, &page_info.row_id, &None, &at, &visit_type, &!is_remote)?;
            // a new visit implies new frecency except in error cases.
            if !visit_ob.is_error.unwrap_or(false) {
                update_frec = true;
            }
            update_change_counter = true;
            Some(row_id)
        }
        None => None,
    };

    if update_change_counter {
        page_info.sync_change_counter += 1;
        updates.push((
            "sync_change_counter",
            ":sync_change_counter",
            &page_info.sync_change_counter,
        ));
    }

    if updates.len() != 0 {
        let mut params: Vec<(&str, &ToSql)> = Vec::with_capacity(updates.len() + 1);
        let mut sets: Vec<String> = Vec::with_capacity(updates.len());
        for (col, name, val) in updates {
            sets.push(format!("{} = {}", col, name));
            params.push((name, val))
        }
        params.push((":row_id", &page_info.row_id.0));
        let sql = format!(
            "UPDATE moz_places
                          SET {}
                          WHERE id == :row_id",
            sets.join(",")
        );
        db.execute_named_cached(&sql, &params)?;
    }
    // This needs to happen after the other updates.
    if update_frec {
        update_frecency(
            &db,
            page_info.row_id,
            Some(visit_ob.get_redirect_frecency_boost()),
        )?;
    }
    Ok(visit_row_id)
}

pub fn update_frecency(db: &Connection, id: RowId, redirect_boost: Option<bool>) -> Result<()> {
    let score = frecency::calculate_frecency(
        db.conn(),
        &frecency::DEFAULT_FRECENCY_SETTINGS,
        id.0, // TODO: calculate_frecency should take a RowId here.
        redirect_boost,
    )?;

    db.execute_named(
        "
        UPDATE moz_places
        SET frecency = :frecency
        WHERE id = :page_id",
        &[(":frecency", &score), (":page_id", &id.0)],
    )?;

    Ok(())
}

// Add a single visit - you must know the page rowid. Does not update the
// page info - if you are calling this, you will also need to update the
// parent page with an updated change counter etc.
fn add_visit(
    db: &impl ConnExt,
    page_id: &RowId,
    from_visit: &Option<RowId>,
    visit_date: &Timestamp,
    visit_type: &VisitTransition,
    is_local: &bool,
) -> Result<RowId> {
    let sql = "INSERT INTO moz_historyvisits
            (from_visit, place_id, visit_date, visit_type, is_local)
        VALUES (:from_visit, :page_id, :visit_date, :visit_type, :is_local)";
    db.execute_named_cached(
        sql,
        &[
            (":from_visit", from_visit),
            (":page_id", page_id),
            (":visit_date", visit_date),
            (":visit_type", visit_type),
            (":is_local", is_local),
        ],
    )?;
    let rid = db.conn().last_insert_rowid();
    Ok(RowId(rid))
}

/// Returns the GUID for the specified Url, or None if it doesn't exist.
pub fn url_to_guid(db: &impl ConnExt, url: &Url) -> Result<Option<SyncGuid>> {
    let sql = "SELECT guid FROM moz_places WHERE url_hash = hash(:url) AND url = :url";
    let result: Option<(SyncGuid)> = db.try_query_row(
        sql,
        &[(":url", &url.clone().into_string())],
        // subtle: we explicitly need to specify rusqlite::Result or the compiler
        // struggles to work out what error type to return from try_query_row.
        |row| -> rusqlite::Result<_> { Ok(row.get_checked::<_, SyncGuid>(0)?) },
        true,
    )?;
    Ok(result)
}

/// Internal function for deleting a place, creating a tombstone if necessary.
/// Assumes a transaction is already set up by the caller.
fn do_delete_place_by_guid(db: &impl ConnExt, guid: &SyncGuid) -> Result<()> {
    // We only create tombstones for history which exists and with sync_status
    // == SyncStatus::Normal
    let sql = "INSERT OR IGNORE INTO moz_places_tombstones (guid)
               SELECT guid FROM moz_places
               WHERE guid = :guid AND sync_status = :status";
    db.execute_named_cached(sql, &[(":guid", guid), (":status", &SyncStatus::Normal)])?;
    // and try the delete - it might not exist, but that's ok.
    let delete_sql = "DELETE FROM moz_places WHERE guid = :guid";
    db.execute_named_cached(delete_sql, &[(":guid", guid)])?;
    Ok(())
}

/// Delete a place given its guid, creating a tombstone if necessary.
pub fn delete_place_by_guid(db: &impl ConnExt, guid: &SyncGuid) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let result = do_delete_place_by_guid(db, guid);
    tx.commit()?;
    result
}

// Support for Sync - in its own module to try and keep a delineation
pub mod history_sync {
    use super::*;
    use crate::history_sync::record::{HistoryRecord, HistoryRecordVisit};
    use crate::history_sync::HISTORY_TTL;
    use std::collections::HashMap;

    #[derive(Debug)]
    pub struct FetchedVisit {
        pub is_local: bool,
        pub visit_date: Timestamp,
        pub visit_type: Option<VisitTransition>,
    }

    impl FetchedVisit {
        pub fn from_row(row: &Row) -> Result<Self> {
            Ok(Self {
                is_local: row.get_checked("is_local")?,
                visit_date: row
                    .get_checked::<_, Option<Timestamp>>("visit_date")?
                    .unwrap_or_default(),
                visit_type: VisitTransition::from_primitive(
                    row.get_checked::<_, Option<u8>>("visit_type")?.unwrap_or(0),
                ),
            })
        }
    }

    #[derive(Debug)]
    pub struct FetchedVisitPage {
        pub url: Url,
        pub guid: SyncGuid,
        pub row_id: RowId,
        pub title: String,
    }

    impl FetchedVisitPage {
        pub fn from_row(row: &Row) -> Result<Self> {
            Ok(Self {
                url: Url::parse(&row.get_checked::<_, String>("url")?)?,
                guid: SyncGuid(row.get_checked::<_, String>("guid")?),
                row_id: row.get_checked("id")?,
                title: row
                    .get_checked::<_, Option<String>>("title")?
                    .unwrap_or_default(),
            })
        }
    }

    pub fn fetch_visits(
        db: &Connection,
        url: &Url,
        limit: usize,
    ) -> Result<Option<(FetchedVisitPage, Vec<FetchedVisit>)>> {
        // We do this in 2 steps - "do we have a page" then "get visits"
        let page_sql = "
          SELECT guid, url, id, title
          FROM moz_places h
          WHERE url_hash = hash(:url) AND url = :url";

        let page_info = match db.try_query_row(
            page_sql,
            &[(":url", &url.to_string())],
            FetchedVisitPage::from_row,
            true,
        )? {
            None => return Ok(None),
            Some(pi) => pi,
        };

        let mut stmt = db.prepare(
            "
          SELECT is_local, visit_type, visit_date
          FROM moz_historyvisits
          WHERE place_id = :place_id
          LIMIT :limit",
        )?;
        let visits = stmt
            .query_and_then_named(
                &[
                    (":place_id", &page_info.row_id),
                    (":limit", &(limit as u32)),
                ],
                FetchedVisit::from_row,
            )?
            .collect::<Result<Vec<_>>>()?;
        Ok(Some((page_info, visits)))
    }

    /// Apply history visit from sync. This assumes they have all been
    /// validated, deduped, etc - it's just the storage we do here.
    pub fn apply_synced_visits(
        db: &Connection,
        incoming_guid: &SyncGuid,
        url: &Url,
        title: &Option<String>,
        visits: &[HistoryRecordVisit],
    ) -> Result<()> {
        let mut counter_incr = 0;
        let page_info = match fetch_page_info(db, &url)? {
            Some(mut info) => {
                // If the existing record has not yet been synced, then we will
                // change the GUID to the incoming one. If it has been synced
                // we keep the existing guid, but still apply the visits.
                // See doc/history_duping.rst for more details.
                if &info.page.guid != incoming_guid {
                    if info.page.sync_status == SyncStatus::New {
                        db.execute_named_cached(
                            "UPDATE moz_places SET guid = :new_guid WHERE id = :row_id",
                            &[(":new_guid", incoming_guid), (":row_id", &info.page.row_id)],
                        )?;
                        info.page.guid = incoming_guid.clone();
                    }
                    // Even if we didn't take the new guid, we are going to
                    // take the new visits - so we want the change counter to
                    // reflect there are changes.
                    counter_incr = 1;
                }
                info.page
            }
            None => new_page_info(db, &url, Some(incoming_guid.clone()))?,
        };
        for visit in visits {
            let transition = VisitTransition::from_primitive(visit.transition)
                .expect("these should already be validated");
            add_visit(
                db,
                &page_info.row_id,
                &None,
                &visit.date.into(),
                &transition,
                &false,
            )?;
        }
        // XXX - we really need a better story for frecency-boost than
        // Option<bool> - None vs Some(false) is confusing. We should use an enum.
        update_frecency(&db, page_info.row_id, None)?;

        // and the place itself if necessary.
        let new_title = title.as_ref().unwrap_or(&page_info.title);
        // We set the Status to Normal, otherwise we will re-upload it as
        // outgoing even if nothing has changed. Note that we *do not* reset
        // the change counter - if it is non-zero now, we want it to remain
        // as non-zero, so we do re-upload it if there were actual changes)
        db.execute_named_cached(
            "UPDATE moz_places
             SET title = :title,
                 sync_status = :status,
                 sync_change_counter = :sync_change_counter
             WHERE id == :row_id",
            &[
                (":title", new_title),
                (":row_id", &page_info.row_id),
                (":status", &SyncStatus::Normal),
                (
                    ":sync_change_counter",
                    &(page_info.sync_change_counter + counter_incr),
                ),
            ],
        )?;

        Ok(())
    }

    pub fn apply_synced_reconciliation(db: &Connection, guid: &SyncGuid) -> Result<()> {
        db.execute_named_cached(
            "UPDATE moz_places
             SET sync_status = :status,
                 sync_change_counter = 0
             WHERE guid == :guid",
            &[(":guid", guid), (":status", &SyncStatus::Normal)],
        )?;
        Ok(())
    }

    pub fn apply_synced_deletion(db: &Connection, guid: &SyncGuid) -> Result<()> {
        // Note that we don't use delete_place_by_guid because we do not want
        // a local tombstone for this item.
        db.execute_named_cached(
            "DELETE FROM moz_places WHERE guid = :guid",
            &[(":guid", guid)],
        )?;
        Ok(())
    }

    #[derive(Debug)]
    pub enum OutgoingInfo {
        Record(HistoryRecord),
        Tombstone,
    }

    pub fn fetch_outgoing(
        db: &Connection,
        max_places: usize,
        max_visits: usize,
    ) -> Result<HashMap<SyncGuid, OutgoingInfo>> {
        // Note that we want *all* "new" regardless of change counter,
        // so that we do the right thing after a "reset".
        let mut stmt = db.conn().prepare(&format!(
            "
            SELECT guid, url, id, title, hidden, typed, frecency,
                visit_count_local, visit_count_remote,
                last_visit_date_local, last_visit_date_remote,
                sync_status, sync_change_counter
            FROM moz_places
            WHERE (sync_change_counter > 0 OR sync_status != {})
            ORDER BY frecency DESC
            LIMIT :max_places",
            (SyncStatus::Normal as u8)
        ))?;
        let mut visits = db.conn().prepare(
            "
            SELECT visit_date as date, visit_type as transition
            FROM moz_historyvisits
            WHERE place_id = :place_id
            ORDER BY visit_date DESC
            LIMIT :max_visits",
        )?;
        // tombstones
        let mut tombstones_stmt = db.conn().prepare(
            "
            SELECT guid FROM moz_places_tombstones LIMIT :max_places",
        )?;

        let mut result: HashMap<SyncGuid, OutgoingInfo> = HashMap::new();

        // We want to limit to 5000 places - tombstones are arguably the
        // most important, so we fetch these first.
        let ts_rows = tombstones_stmt.query_and_then_named(
            &[(":max_places", &(max_places as u32))],
            |row| -> rusqlite::Result<_> { Ok(SyncGuid(row.get_checked::<_, String>("guid")?)) },
        )?;
        for r in ts_rows {
            let guid = r?;
            log::trace!("outgoing tombstone {:?}", &guid);
            result.insert(guid, OutgoingInfo::Tombstone);
        }

        // Max records is now limited by how many tombstones we found.
        let max_places_left = max_places - result.len();

        // We write info about the records we are updating to a temp table.
        // While we could carry this around in memory, we'll need a temp table
        // in `finish_outgoing` anyway, because we execute a `NOT IN` query
        // there - which, in a worst-case scenario, is a very large `NOT IN`
        // set.
        db.execute(
            "CREATE TEMP TABLE IF NOT EXISTS temp_sync_updated_meta
                    (id INTEGER PRIMARY KEY,
                     change_delta INTEGER NOT NULL)",
            NO_PARAMS,
        )?;

        let insert_meta_sql = "
            INSERT INTO temp_sync_updated_meta VALUES (:row_id, :change_delta)";

        let rows = stmt.query_and_then_named(
            &[(":max_places", &(max_places_left as u32))],
            PageInfo::from_row,
        )?;
        let mut ids_to_update = Vec::new();
        for t in rows {
            let page = t?;
            let visit_rows = visits.query_and_then_named(
                &[
                    (":max_visits", &(max_visits as u32)),
                    (":place_id", &page.row_id),
                ],
                |row| {
                    Ok(HistoryRecordVisit {
                        date: row.get_checked::<_, Timestamp>("date")?.into(),
                        transition: row.get_checked::<_, u8>("transition")?,
                    })
                },
            )?;
            let visits = visit_rows.collect::<RusqliteResult<Vec<_>>>()?;
            if result.contains_key(&page.guid) {
                // should be impossible!
                log::warn!("Found {:?} in both tombstones and live records", &page.guid);
                continue;
            }
            if visits.len() == 0 {
                log::info!(
                    "Page {:?} is flagged to be uploaded, but has no visits - skipping",
                    &page.guid
                );
                continue;
            }
            log::trace!("outgoing record {:?}", &page.guid);
            ids_to_update.push(page.row_id);
            db.execute_named_cached(
                insert_meta_sql,
                &[
                    (":row_id", &page.row_id),
                    (":change_delta", &page.sync_change_counter),
                ],
            )?;

            result.insert(
                page.guid.clone(),
                OutgoingInfo::Record(HistoryRecord {
                    id: page.guid,
                    title: page.title,
                    hist_uri: page.url.to_string(),
                    sortindex: page.frecency,
                    ttl: HISTORY_TTL,
                    visits,
                }),
            );
        }

        // We need to update the sync status of these items now rather than after
        // the upload, because if we are interrupted between upload and writing
        // we could end up with local items with state New even though we
        // uploaded them.
        sql_support::each_chunk(&ids_to_update, |chunk, _| -> Result<()> {
            db.conn().execute(
                &format!(
                    "UPDATE moz_places SET sync_status={status}
                                 WHERE id IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len()),
                    status = SyncStatus::Normal as u8
                ),
                chunk,
            )?;
            Ok(())
        })?;

        Ok(result)
    }

    pub fn finish_outgoing(db: &Connection) -> Result<()> {
        // So all items *other* than those above must be set to "not dirty"
        // (ie, status=SyncStatus::Normal, change_counter=0). Otherwise every
        // subsequent sync will continue to add more and more local pages
        // until every page we have is uploaded. And we only want to do it
        // at the end of the sync because if we are interrupted, we'll end up
        // thinking we have nothing to upload.
        // BUT - this is potentially alot of rows! Because we want "NOT IN (...)"
        // we can't do chunking and building a literal string with the ids seems
        // wrong and likely to hit max sql length limits.
        // So we use a temp table.
        log::debug!("Updating all synced rows");
        // XXX - is there a better way to express this SQL? Multi-selects
        // doesn't seem ideal...
        db.conn().execute_cached(
            "
            UPDATE moz_places
            SET sync_change_counter = sync_change_counter -
                (SELECT change_delta FROM temp_sync_updated_meta m WHERE moz_places.id = m.id)
            WHERE id IN (SELECT id FROM temp_sync_updated_meta)
            ",
            NO_PARAMS,
        )?;

        log::debug!("Updating all non-synced rows");
        db.execute_all(&[
            &format!(
                "UPDATE moz_places
                                   SET sync_change_counter = 0, sync_status = {}
                                   WHERE id NOT IN (SELECT id from temp_sync_updated_meta)",
                (SyncStatus::Normal as u8)
            ),
            "DELETE FROM temp_sync_updated_meta",
        ])?;

        log::debug!("Removing local tombstones");
        db.conn()
            .execute_cached("DELETE from moz_places_tombstones", NO_PARAMS)?;

        Ok(())
    }

    pub fn reset_storage(db: &Connection) -> Result<()> {
        db.conn().execute_cached(
            &format!(
                "
                UPDATE moz_places
                SET sync_change_counter = 0,
                    sync_status = {}",
                (SyncStatus::New as u8)
            ),
            NO_PARAMS,
        )?;
        Ok(())
    }
} // end of sync module.

pub fn get_visited<I>(db: &PlacesDb, urls: I) -> Result<Vec<bool>>
where
    I: IntoIterator<Item = Url>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = urls.into_iter();
    let mut result = vec![false; iter.len()];
    let url_idxs = iter.enumerate().collect::<Vec<_>>();
    get_visited_into(db, &url_idxs, &mut result)?;
    Ok(result)
}

/// Low level api used to implement both get_visited and the FFI get_visited call.
/// Takes a slice where we should output the results, as well as a slice of
/// index/url pairs.
///
/// This is done so that the FFI can more easily support returning
/// false when asked if it's visited an invalid URL.
pub fn get_visited_into(
    db: &PlacesDb,
    urls_idxs: &[(usize, Url)],
    result: &mut [bool],
) -> Result<()> {
    sql_support::each_chunk_mapped(
        &urls_idxs,
        |(_, url)| url.as_str(),
        |chunk, offset| -> Result<()> {
            let values_with_idx = sql_support::repeat_display(chunk.len(), ",", |i, f| {
                let (idx, url) = &urls_idxs[i + offset];
                write!(f, "({},{},?)", *idx, hash::hash_url(url.as_str()))
            });
            let sql = format!(
                "WITH to_fetch(fetch_url_index, url_hash, url) AS (VALUES {})
                 SELECT fetch_url_index
                 FROM moz_places h
                 JOIN to_fetch f ON h.url_hash = f.url_hash
                   AND h.url = f.url",
                values_with_idx
            );
            let mut stmt = db.prepare(&sql)?;
            for idx_r in stmt.query_and_then(chunk, |row| -> rusqlite::Result<_> {
                Ok(row.get_checked::<_, i64>(0)? as usize)
            })? {
                let idx = idx_r?;
                result[idx] = true;
            }
            Ok(())
        },
    )?;
    Ok(())
}

/// Get the set of urls that were visited between `start` and `end`. Only considers local visits
/// unless you pass in `include_remote`.
pub fn get_visited_urls(
    db: &PlacesDb,
    start: Timestamp,
    end: Timestamp,
    include_remote: bool,
) -> Result<Vec<String>> {
    // TODO: if `end` is >= now then we can probably just look at last_visit_date_{local,remote},
    // and avoid touching `moz_historyvisits` at all. That said, this query is taken more or less
    // from what places does so it's probably fine.
    let mut stmt = db.prepare(&format!(
        "
        SELECT h.url
        FROM moz_places h
        WHERE EXISTS (
            SELECT 1 FROM moz_historyvisits v
            WHERE place_id = h.id
                AND visit_date BETWEEN :start AND :end
                {and_is_local}
            LIMIT 1
        )
    ",
        and_is_local = if include_remote { "" } else { "AND is_local" }
    ))?;

    let iter = stmt.query_and_then_named(&[(":start", &start), (":end", &end)], |row| {
        Ok(row.get_checked::<_, String>(0)?)
    })?;

    Ok(iter.collect::<RusqliteResult<Vec<_>>>()?)
}

// Mini experiment with an "Origin" object that knows how to rev_host() itself,
// that I don't want to throw away yet :) I'm really not sure exactly how
// moz_origins fits in TBH :/
#[cfg(test)]
mod tests {
    use super::history_sync::*;
    use super::*;
    use crate::history_sync::record::HistoryRecord;
    use std::time::{Duration, SystemTime};

    struct Origin {
        prefix: String,
        host: String,
        frecency: i64,
    }
    impl Origin {
        pub fn rev_host(&self) -> String {
            // Note: this is consistent with how places handles hosts, and our `reverse_host`
            // function. We explictly don't want to use unicode_segmentation because it's not
            // stable across unicode versions, and valid hosts are expected to be strings.
            // (The `url` crate will punycode them for us).
            String::from_utf8(
                self.host
                    .bytes()
                    .rev()
                    .map(|b| b.to_ascii_lowercase())
                    .collect::<Vec<_>>(),
            )
            .unwrap() // TODO: We should return a Result, or punycode on construction if needed.
        }
    }

    #[test]
    fn test_reverse() {
        let o = Origin {
            prefix: "http".to_string(),
            host: "foo.com".to_string(),
            frecency: 0,
        };
        assert_eq!(o.prefix, "http");
        assert_eq!(o.frecency, 0);
        assert_eq!(o.rev_host(), "moc.oof");
    }

    #[test]
    fn test_get_visited_urls() {
        use std::collections::HashSet;
        use std::time::SystemTime;
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let now: Timestamp = SystemTime::now().into();
        let now_u64 = now.0;
        // (url, when, is_remote, (expected_always, expected_only_local)
        let to_add = [
            (
                "https://www.example.com/1",
                now_u64 - 200100,
                false,
                (false, false),
            ),
            (
                "https://www.example.com/12",
                now_u64 - 200000,
                false,
                (true, true),
            ),
            (
                "https://www.example.com/123",
                now_u64 - 10000,
                true,
                (true, false),
            ),
            (
                "https://www.example.com/1234",
                now_u64 - 1000,
                false,
                (true, true),
            ),
            (
                "https://www.mozilla.com",
                now_u64 - 500,
                false,
                (false, false),
            ),
        ];

        for &(url, when, remote, _) in &to_add {
            apply_observation(
                &mut conn,
                VisitObservation::new(Url::parse(url).unwrap())
                    .with_at(Timestamp(when))
                    .with_is_remote(remote)
                    .with_visit_type(VisitTransition::Link),
            )
            .expect("Should apply visit");
        }

        let visited_all = get_visited_urls(
            &conn,
            Timestamp(now_u64 - 200000),
            Timestamp(now_u64 - 1000),
            true,
        )
        .unwrap()
        .into_iter()
        .collect::<HashSet<_>>();

        let visited_local = get_visited_urls(
            &conn,
            Timestamp(now_u64 - 200000),
            Timestamp(now_u64 - 1000),
            false,
        )
        .unwrap()
        .into_iter()
        .collect::<HashSet<_>>();

        for &(url, ts, is_remote, (expected_in_all, expected_in_local)) in &to_add {
            // Make sure we format stuff the same way (in practice, just trailing slashes)
            let url = Url::parse(url).unwrap().to_string();
            assert_eq!(
                expected_in_local,
                visited_local.contains(&url),
                "Failed in local for {:?}",
                (url, ts, is_remote)
            );
            assert_eq!(
                expected_in_all,
                visited_all.contains(&url),
                "Failed in all for {:?}",
                (url, ts, is_remote)
            );
        }
    }

    fn get_custom_observed_page<F>(conn: &mut PlacesDb, url: &str, custom: F) -> Result<PageInfo>
    where
        F: Fn(VisitObservation) -> VisitObservation,
    {
        let u = Url::parse(url)?;
        let obs = VisitObservation::new(u.clone()).with_visit_type(VisitTransition::Link);
        apply_observation(conn, custom(obs))?;
        Ok(fetch_page_info(conn.conn(), &u)?
            .expect("should have the page")
            .page)
    }

    fn get_observed_page(conn: &mut PlacesDb, url: &str) -> Result<PageInfo> {
        get_custom_observed_page(conn, url, |o| o)
    }

    fn get_tombstone_count(conn: &PlacesDb) -> u32 {
        let result: Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_places_tombstones;",
            &[],
            |row| Ok(row.get_checked::<_, u32>(0)?.clone()),
            true,
        );
        result
            .expect("should have worked")
            .expect("should have got a value")
            .into()
    }

    #[test]
    fn test_visit_counts() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://www.example.com").expect("it's a valid url");
        let early_time = SystemTime::now() - Duration::new(60, 0);
        let late_time = SystemTime::now();

        // add 2 local visits - add latest first
        let rid1 = apply_observation(
            &mut conn,
            VisitObservation::new(url.clone())
                .with_visit_type(VisitTransition::Link)
                .with_at(Some(late_time.into())),
        )?
        .expect("should get a rowid");

        let rid2 = apply_observation(
            &mut conn,
            VisitObservation::new(url.clone())
                .with_visit_type(VisitTransition::Link)
                .with_at(Some(early_time.into())),
        )?
        .expect("should get a rowid");

        let mut pi = fetch_page_info(&conn, &url)?.expect("should have the page");
        assert_eq!(pi.page.visit_count_local, 2);
        assert_eq!(pi.page.last_visit_date_local, late_time.into());
        assert_eq!(pi.page.visit_count_remote, 0);
        assert_eq!(pi.page.last_visit_date_remote.0, 0);

        // 2 remote visits, earliest first.
        let rid3 = apply_observation(
            &mut conn,
            VisitObservation::new(url.clone())
                .with_visit_type(VisitTransition::Link)
                .with_at(Some(early_time.into()))
                .with_is_remote(true),
        )?
        .expect("should get a rowid");

        let rid4 = apply_observation(
            &mut conn,
            VisitObservation::new(url.clone())
                .with_visit_type(VisitTransition::Link)
                .with_at(Some(late_time.into()))
                .with_is_remote(true),
        )?
        .expect("should get a rowid");

        pi = fetch_page_info(&conn, &url)?.expect("should have the page");
        assert_eq!(pi.page.visit_count_local, 2);
        assert_eq!(pi.page.last_visit_date_local, late_time.into());
        assert_eq!(pi.page.visit_count_remote, 2);
        assert_eq!(pi.page.last_visit_date_remote, late_time.into());

        // Delete some and make sure things update.
        // XXX - we should add a trigger to update frecency on delete, but at
        // this stage we don't "officially" support deletes, so this is TODO.
        let sql = "DELETE FROM moz_historyvisits WHERE id = :row_id";
        // Delete the latest local visit.
        conn.execute_named_cached(&sql, &[(":row_id", &rid1)])?;
        pi = fetch_page_info(&conn, &url)?.expect("should have the page");
        assert_eq!(pi.page.visit_count_local, 1);
        assert_eq!(pi.page.last_visit_date_local, early_time.into());
        assert_eq!(pi.page.visit_count_remote, 2);
        assert_eq!(pi.page.last_visit_date_remote, late_time.into());

        // Delete the earliest remote  visit.
        conn.execute_named_cached(&sql, &[(":row_id", &rid3)])?;
        pi = fetch_page_info(&conn, &url)?.expect("should have the page");
        assert_eq!(pi.page.visit_count_local, 1);
        assert_eq!(pi.page.last_visit_date_local, early_time.into());
        assert_eq!(pi.page.visit_count_remote, 1);
        assert_eq!(pi.page.last_visit_date_remote, late_time.into());

        // Delete all visits.
        conn.execute_named_cached(&sql, &[(":row_id", &rid2)])?;
        conn.execute_named_cached(&sql, &[(":row_id", &rid4)])?;
        // It may turn out that we also delete the place after deleting all
        // visits, but for now we don't - check the values are sane though.
        pi = fetch_page_info(&conn, &url)?.expect("should have the page");
        assert_eq!(pi.page.visit_count_local, 0);
        assert_eq!(pi.page.last_visit_date_local, Timestamp(0).into());
        assert_eq!(pi.page.visit_count_remote, 0);
        assert_eq!(pi.page.last_visit_date_remote, Timestamp(0).into());
        Ok(())
    }

    #[test]
    fn test_get_visited() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;

        let unicode_in_path = "http://www.example.com/tëst😀abc";
        let escaped_unicode_in_path = "http://www.example.com/t%C3%ABst%F0%9F%98%80abc";

        let unicode_in_domain = "http://www.exämple😀123.com";
        let escaped_unicode_in_domain = "http://www.xn--exmple123-w2a24222l.com";

        let to_add = [
            "https://www.example.com/1".to_string(),
            "https://www.example.com/12".to_string(),
            "https://www.example.com/123".to_string(),
            "https://www.example.com/1234".to_string(),
            "https://www.mozilla.com".to_string(),
            "https://www.firefox.com".to_string(),
            unicode_in_path.to_string() + "/1",
            escaped_unicode_in_path.to_string() + "/2",
            unicode_in_domain.to_string() + "/1",
            escaped_unicode_in_domain.to_string() + "/2",
        ];

        for item in &to_add {
            apply_observation(
                &mut conn,
                VisitObservation::new(Url::parse(item).unwrap())
                    .with_visit_type(VisitTransition::Link),
            )?;
        }

        let to_search = [
            ("https://www.example.com".to_string(), false),
            ("https://www.example.com/1".to_string(), true),
            ("https://www.example.com/12".to_string(), true),
            ("https://www.example.com/123".to_string(), true),
            ("https://www.example.com/1234".to_string(), true),
            ("https://www.example.com/12345".to_string(), false),
            ("https://www.mozilla.com".to_string(), true),
            ("https://www.firefox.com".to_string(), true),
            ("https://www.mozilla.org".to_string(), false),
            // dupes should still work!
            ("https://www.example.com/1234".to_string(), true),
            ("https://www.example.com/12345".to_string(), false),
            // The unicode URLs should work when escaped the way we
            // encountered them
            (unicode_in_path.to_string() + "/1", true),
            (escaped_unicode_in_path.to_string() + "/2", true),
            (unicode_in_domain.to_string() + "/1", true),
            (escaped_unicode_in_domain.to_string() + "/2", true),
            // But also the other way.
            (unicode_in_path.to_string() + "/2", true),
            (escaped_unicode_in_path.to_string() + "/1", true),
            (unicode_in_domain.to_string() + "/2", true),
            (escaped_unicode_in_domain.to_string() + "/1", true),
        ];

        let urls = to_search
            .iter()
            .map(|(url, _expect)| Url::parse(&url).unwrap())
            .collect::<Vec<_>>();

        let visited = get_visited(&conn, urls).unwrap();

        assert_eq!(visited.len(), to_search.len());

        for (i, &did_see) in visited.iter().enumerate() {
            assert_eq!(
                did_see,
                to_search[i].1,
                "Wrong value in get_visited for '{}' (idx {}), want {}, have {}",
                to_search[i].0,
                i, // idx is logged because some things are repeated
                to_search[i].1,
                did_see
            );
        }
        Ok(())
    }

    #[test]
    fn test_get_visited_into() {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None).unwrap();

        let to_add = [
            Url::parse("https://www.example.com/1").unwrap(),
            Url::parse("https://www.example.com/12").unwrap(),
            Url::parse("https://www.example.com/123").unwrap(),
        ];

        for item in &to_add {
            apply_observation(
                &mut conn,
                VisitObservation::new(item.clone()).with_visit_type(VisitTransition::Link),
            )
            .unwrap();
        }

        let mut results = [false; 10];

        let get_visited_request = [
            // 0 blank
            (2, to_add[1].clone()),
            (1, to_add[0].clone()),
            // 3 blank
            (4, to_add[2].clone()),
            // 5 blank
            // Note: url for 6 is not visited.
            (6, Url::parse("https://www.example.com/1234").unwrap()),
            // 7 blank
            // Note: dupe is allowed
            (8, to_add[1].clone()),
            // 9 is blank
        ];

        get_visited_into(&conn, &get_visited_request, &mut results).unwrap();
        let expect = [
            false, // 0
            true,  // 1
            true,  // 2
            false, // 3
            true,  // 4
            false, // 5
            false, // 6
            false, // 7
            true,  // 8
            false, // 9
        ];

        assert_eq!(expect, results);
    }

    #[test]
    fn test_change_counter() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let mut pi = get_observed_page(&mut conn, "http://example.com")?;
        // A new observation with just a title (ie, no visit) should update it.
        apply_observation(
            &mut conn,
            VisitObservation::new(pi.url.clone()).with_title(Some("new title".into())),
        )?;
        pi = fetch_page_info(&conn, &pi.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi.title, "new title");
        assert_eq!(pi.sync_change_counter, 2);
        Ok(())
    }

    #[test]
    fn test_status_columns() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let _ = env_logger::try_init();
        // A page with "normal" and a change counter.
        let mut pi = get_observed_page(&mut conn, "http://example.com/1")?;
        assert_eq!(pi.sync_change_counter, 1);
        conn.execute_named_cached(
            "UPDATE moz_places
                                   SET frecency = 100
                                   WHERE id = :id",
            &[(":id", &pi.row_id)],
        )?;
        // A page with "new" and no change counter.
        let mut pi2 = get_observed_page(&mut conn, "http://example.com/2")?;
        conn.execute_named_cached(
            "UPDATE moz_places
                                   SET sync_status = :status,
                                       sync_change_counter = 0,
                                       frecency = 50
                                   WHERE id = :id",
            &[(":status", &(SyncStatus::New as u8)), (":id", &pi2.row_id)],
        )?;

        // A second page with "new", a change counter (which will be ignored
        // as we will limit such that this isn't sent) and a low frecency.
        let mut pi3 = get_observed_page(&mut conn, "http://example.com/3")?;
        conn.execute_named_cached(
            "UPDATE moz_places
                                   SET sync_status = :status,
                                       sync_change_counter = 1,
                                       frecency = 10
                                   WHERE id = :id",
            &[(":status", &(SyncStatus::New as u8)), (":id", &pi3.row_id)],
        )?;

        let mut outgoing = fetch_outgoing(&conn, 2, 3)?;
        assert_eq!(outgoing.len(), 2, "should have restricted to the limit");
        // I'm sure there's a shorter way to express this...
        let mut records: Vec<HistoryRecord> = Vec::with_capacity(outgoing.len());
        for (_, outgoing) in outgoing.drain() {
            records.push(match outgoing {
                OutgoingInfo::Record(record) => record,
                _ => continue,
            });
        }
        // want p1 or pi1 (but order is indeterminate)
        assert!(records[0].id != records[1].id);
        assert!(records[0].id == pi.guid || records[0].id == pi2.guid);
        assert!(records[1].id == pi.guid || records[1].id == pi2.guid);
        finish_outgoing(&conn)?;

        pi = fetch_page_info(&conn, &pi.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi.sync_change_counter, 0);
        pi2 = fetch_page_info(&conn, &pi2.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi2.sync_change_counter, 0);
        assert_eq!(pi2.sync_status, SyncStatus::Normal);

        // pi3 wasn't uploaded, but it should still have been changed to
        // Normal and had the change counter reset.
        pi3 = fetch_page_info(&conn, &pi3.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi3.sync_change_counter, 0);
        assert_eq!(pi3.sync_status, SyncStatus::Normal);
        Ok(())
    }

    #[test]
    fn test_tombstones() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitTransition::Link)
            .with_at(Some(SystemTime::now().into()));
        apply_observation(&mut db, obs)?;
        let guid = url_to_guid(&db, &url)?.expect("should exist");

        delete_place_by_guid(&db, &guid)?;

        // status was "New", so expect no tombstone.
        assert_eq!(get_tombstone_count(&db), 0);

        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitTransition::Link)
            .with_at(Some(SystemTime::now().into()));
        apply_observation(&mut db, obs)?;
        let new_guid = url_to_guid(&db, &url)?.expect("should exist");

        // Set the status to normal
        db.execute_named_cached(
            &format!(
                "UPDATE moz_places
                 SET sync_status = {}
                 WHERE guid = :guid",
                (SyncStatus::Normal as u8)
            ),
            &[(":guid", &new_guid)],
        )?;
        delete_place_by_guid(&db, &new_guid)?;
        assert_eq!(get_tombstone_count(&db), 1);
        Ok(())
    }

    #[test]
    fn test_sync_reset() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let _ = env_logger::try_init();
        let mut pi = get_observed_page(&mut conn, "http://example.com")?;
        conn.execute_cached(
            &format!(
                "UPDATE moz_places set sync_status = {}",
                (SyncStatus::Normal as u8)
            ),
            NO_PARAMS,
        )?;
        pi = fetch_page_info(&conn, &pi.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi.sync_change_counter, 1);
        assert_eq!(pi.sync_status, SyncStatus::Normal);
        reset_storage(&conn)?;
        pi = fetch_page_info(&conn, &pi.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi.sync_change_counter, 0);
        assert_eq!(pi.sync_status, SyncStatus::New);
        // Ensure we are going to do a full re-upload after a reset.
        let outgoing = fetch_outgoing(&conn, 100, 100)?;
        assert_eq!(outgoing.len(), 1);
        Ok(())
    }

    #[test]
    fn test_fetch_visits() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let pi = get_observed_page(&mut conn, "http://example.com/1")?;
        assert_eq!(fetch_visits(&conn, &pi.url, 0).unwrap().unwrap().1.len(), 0);
        assert_eq!(fetch_visits(&conn, &pi.url, 1).unwrap().unwrap().1.len(), 1);
        Ok(())
    }

    #[test]
    fn test_apply_synced_reconciliation() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let mut pi = get_observed_page(&mut conn, "http://example.com/1")?;
        assert_eq!(pi.sync_status, SyncStatus::New);
        assert_eq!(pi.sync_change_counter, 1);
        apply_synced_reconciliation(&conn, &pi.guid)?;
        pi = fetch_page_info(&conn, &pi.url)?
            .expect("page should exist")
            .page;
        assert_eq!(pi.sync_status, SyncStatus::Normal);
        assert_eq!(pi.sync_change_counter, 0);
        Ok(())
    }

    #[test]
    fn test_apply_synced_deletion_new() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let pi = get_observed_page(&mut conn, "http://example.com/1")?;
        assert_eq!(pi.sync_status, SyncStatus::New);
        apply_synced_deletion(&conn, &pi.guid)?;
        assert!(
            fetch_page_info(&conn, &pi.url)?.is_none(),
            "should have been deleted"
        );
        assert_eq!(get_tombstone_count(&conn), 0, "should be no tombstones");
        Ok(())
    }

    #[test]
    fn test_apply_synced_deletion_normal() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None)?;
        let pi = get_observed_page(&mut conn, "http://example.com/1")?;
        assert_eq!(pi.sync_status, SyncStatus::New);
        conn.execute_cached(
            &format!(
                "UPDATE moz_places set sync_status = {}",
                (SyncStatus::Normal as u8)
            ),
            NO_PARAMS,
        )?;

        apply_synced_deletion(&conn, &pi.guid)?;
        assert!(
            fetch_page_info(&conn, &pi.url)?.is_none(),
            "should have been deleted"
        );
        assert_eq!(get_tombstone_count(&conn), 0, "should be no tombstones");
        Ok(())
    }
}
