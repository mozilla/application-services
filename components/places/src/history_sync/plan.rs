/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection};
use sql_support::ConnExt;
use url::{Url};

use super::{MAX_OUTGOING_PLACES, MAX_VISITS, HISTORY_TTL};
use super::record::{HistoryRecord, HistoryRecordVisit, HistorySyncRecord};
use types::{SyncGuid, Timestamp, VisitTransition};
use storage::history_sync::{
    apply_synced_deletion,
    apply_synced_visits,
    apply_synced_reconcilliation,
    fetch_outgoing,
    fetch_visits,
    FetchedVisit,
    FetchedVisitPage,
    finish_outgoing,
    OutgoingInfo,
};
use valid_guid::{is_valid_places_guid};

use api::history::{can_add_url};
use error::*;

use sync15_adapter::{
    IncomingChangeset,
    OutgoingChangeset,
    Payload,
};

// In desktop sync, bookmarks are clamped to Jan 23, 1993 (which is 727747200000)
// There's no good reason history records could be older than that, so we do
// the same here (even though desktop's history currently doesn't)
// XXX - there's probably a case to be made for this being, say, 5 years ago -
// then all requests earlier than that are collapsed into a single visit at
// this timestamp.
const EARLIEST_TIMESTAMP: Timestamp = Timestamp(727747200000);

/// Clamps a history visit date between the current date and the earliest
/// sensible date.
fn clamp_visit_date(visit_date: Timestamp) -> Timestamp {
    let now = Timestamp::now();
    if visit_date > now {
        return visit_date;
    }
    if visit_date < EARLIEST_TIMESTAMP {
        return EARLIEST_TIMESTAMP;
    }
    return visit_date;
}

#[derive(Debug)]
pub enum IncomingPlan {
    Skip, // An entry we just want to ignore - either due to the URL etc, or because no changes.
    Invalid(Error), // Something's wrong with this entry.
    Failed(Error), // The entry appears sane, but there was some error.
    Delete, // We should delete this.
    // We should apply this. If SyncGuid is Some, then it is the existing guid
    // for the same URL, and if that doesn't match the incoming record, we
    // should change the existing item to the incoming one and upload a tombstone.
    Apply(Option<SyncGuid>, Url, Option<String>, Vec<HistoryRecordVisit>),
    Reconciled, // Entry exists locally and it's the same as the incoming record.
}


fn plan_incoming_record(conn: &Connection, record: HistoryRecord, max_visits: usize) -> IncomingPlan {
    let url = match Url::parse(&record.hist_uri) {
        Ok(u) => u,
        Err(e) => return IncomingPlan::Invalid(e.into()),
    };

    if !is_valid_places_guid(record.id.as_ref()) {
        return IncomingPlan::Invalid(InvalidPlaceInfo::InvalidGuid.into());
    }

    match can_add_url(&url) {
        Ok(can) => if !can { return IncomingPlan::Skip },
        Err(e) => return IncomingPlan::Failed(e.into()),
    }
    // Let's get what we know about it, if anything - last 20, like desktop?
    let visit_tuple = match fetch_visits(conn, &url, max_visits) {
        Ok(v) => v,
        Err(e) => return IncomingPlan::Failed(e.into()),
    };

    // This all seems more messy than it should be - struggling to find the
    // correct signature for fetch_visits
    let (existing_page, existing_visits): (Option<FetchedVisitPage>, Vec<FetchedVisit>) = match visit_tuple {
        None => (None, Vec::new()),
        Some((p, v)) => (Some(p), v),
    };

    let mut old_guid: Option<SyncGuid> = None;
    let mut cur_visit_map: HashSet<(VisitTransition, Timestamp)> = HashSet::with_capacity(existing_visits.len());
    if let Some(p) = &existing_page {
        if p.guid != record.id {
            old_guid = Some(p.guid.clone());
        }
        for visit in &existing_visits {
            // it should be impossible for us to have invalid visits locally, but...
            let transition = match visit.visit_type {
                Some(t) => t,
                None => continue,
            };
            let date_use = clamp_visit_date(visit.visit_date);
            cur_visit_map.insert((transition, date_use));
        }
    }
    // If we already have MAX_RECORDS visits, then we will ignore incoming
    // visits older than that. (Not really clear why we do this and why 20 is
    // magic, but what's good enough for desktop is good enough for us at this
    // stage.)
    let earliest_allowed: SystemTime = if existing_visits.len() == max_visits as usize {
        existing_visits[existing_visits.len() - 1].visit_date.into()
    } else {
        UNIX_EPOCH
    };

    // work out which of the incoming records we should apply.
    let mut to_apply = Vec::with_capacity(record.visits.len());
    for incoming_visit in record.visits {
        let transition = match VisitTransition::from_primitive(incoming_visit.transition) {
            Some(v) => v,
            None => continue,
        };
        let timestamp = clamp_visit_date(incoming_visit.date);
        if earliest_allowed > timestamp.into() {
            continue;
        }
        // If the entry isn't in our map we should add it.
        let key = (transition, timestamp);
        if !cur_visit_map.contains(&key) {
            to_apply.push(HistoryRecordVisit { date: timestamp, transition: transition as u8 });
            cur_visit_map.insert(key);
        }
    }
    if to_apply.len() != 0 || old_guid.is_some() {
        // Now we need to check the other attributes.
        // Check if we should update title? For now, assume yes. It appears
        // as though desktop always updates it.
        let new_title = Some(record.title);
        IncomingPlan::Apply(old_guid, url.clone(), new_title, to_apply)
    } else {
        IncomingPlan::Reconciled
    }
}

pub fn apply_plan(conn: &Connection, inbound: IncomingChangeset) -> Result<OutgoingChangeset> {
    // for a first-cut, let's do this in the most naive way possible...
    let mut plans: Vec<(SyncGuid, IncomingPlan)> = Vec::with_capacity(inbound.changes.len());
    for incoming in inbound.changes {
        let item = HistorySyncRecord::from_payload(incoming.0)?;
        let plan = match item.record {
            Some(record) => plan_incoming_record(conn, record, MAX_VISITS),
            None => IncomingPlan::Delete,
        };
        let guid = item.guid.clone();
        plans.push((guid, plan));
    }

    let tx = conn.unchecked_transaction()?;
    let mut num_applied = 0;
    let mut num_deleted = 0;
    let mut num_reconciled = 0;

    let mut outgoing = OutgoingChangeset::new("history".into(), inbound.timestamp);
    for (guid, plan) in plans {
        match &plan {
            IncomingPlan::Skip => {
                trace!("incoming: skipping item {:?}", guid);
            },
            IncomingPlan::Invalid(err) => {
                warn!("incoming: record {:?} skipped because it is invalid: {}", guid, err);
            },
            IncomingPlan::Failed(err) => {
                error!("incoming: record {:?} failed to apply: {}", guid, err);
            },
            IncomingPlan::Delete => {
                trace!("incoming: deleting {:?}", guid);
                num_deleted += 1;
                apply_synced_deletion(&conn, &guid)?;
            },
            IncomingPlan::Apply(old_guid, url, title, visits_to_add) => {
                num_applied += 1;
                trace!("incoming: will apply {:?}: url={:?}, title={:?}, to_add={:?}",
                      guid, url, title, visits_to_add);
                apply_synced_visits(&conn, &guid, &old_guid, &url, title, visits_to_add)?;
                if let Some(ref old_guid) = old_guid {
                    outgoing.changes.push(Payload::new_tombstone(old_guid.0.clone()));
                }
            },
            IncomingPlan::Reconciled => {
                num_reconciled += 1;
                trace!("incoming: reconciled {:?}", guid);
                apply_synced_reconcilliation(&conn, &guid)?;
            },
        };
    }
    // XXX - we could probably commit the transaction here and start a new
    // one? OTOH, we might look at killing all transactions here?
    let mut out_infos = fetch_outgoing(conn, MAX_OUTGOING_PLACES, MAX_VISITS)?;

    for (guid, out_record) in out_infos.drain() {
        let payload = match out_record {
            OutgoingInfo::Record(record) => {
                // XXX - comments in `from_record` imply this is dumb. I'm
                // really not sure I'm using `Payload` correctly?
                Payload::from_record(record)?
            },
            // XXX - we need a way to ensure the TTL is set on the tombstone?
            OutgoingInfo::Tombstone => Payload::new_tombstone_with_ttl(guid.0.clone(),
                                                                       HISTORY_TTL),
        };
        trace!("outgoing {:?}", payload);
        outgoing.changes.push(payload);
    }
    tx.commit()?;

    info!("incoming: applied {}, deleted {}, reconciled {}", num_applied, num_deleted, num_reconciled);

    Ok(outgoing)
}

pub fn finish_plan(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    finish_outgoing(conn)?;
    trace!("Committing final sync plan");
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration};
    use storage::{apply_observation};
    use storage::history_sync::{fetch_visits};
    use observation::{VisitObservation};
    use api::matcher::{SearchParams, search_frecent};
    use types::{Timestamp, SyncStatus};
    use db::PlacesDb;

    use sql_support::ConnExt;
    use sync15_adapter::util::{random_guid};

    use sync15_adapter::{IncomingChangeset, ServerTimestamp};
    use url::{Url};
    use env_logger;

    fn get_existing_guid(conn: &PlacesDb, url: &Url) -> SyncGuid {
        let guid_result: Result<Option<String>> = conn.try_query_row(
                    "SELECT guid from moz_places WHERE url = :url;",
                    &[(":url", &url.clone().into_string())],
                    |row| Ok(row.get::<_, String>(0).clone()), true);
        guid_result.expect("should have worked").expect("should have got a value").into()
    }

    fn get_tombstone_count(conn: &PlacesDb) -> u32 {
        let result: Result<Option<u32>> = conn.try_query_row(
                        "SELECT COUNT(*) from moz_places_tombstones;",
                        &[],
                        |row| Ok(row.get::<_, u32>(0).clone()), true);
        result.expect("should have worked").expect("should have got a value").into()
    }

    fn get_sync(conn: &PlacesDb, url: &Url) -> (SyncStatus, u32) {
        let guid_result: Result<Option<(SyncStatus, u32)>> = conn.try_query_row(
                    "SELECT sync_status, sync_change_counter
                     FROM moz_places
                     WHERE url = :url;",
                    &[(":url", &url.clone().into_string())],
                    |row| Ok((SyncStatus::from_u8(row.get::<_, u8>(0)), row.get::<_, u32>(1))), true);
        guid_result.expect("should have worked").expect("should have got values").into()
    }

    #[test]
    fn test_invalid_guid() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let record = HistoryRecord { id: SyncGuid("foo".to_string()),
                                     title: "title".into(),
                                     hist_uri: "http://example.com".into(),
                                     sortindex: 0,
                                     ttl: 100,
                                     visits: vec![]};

        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Invalid(_) => true,
            _ => false
        });
        Ok(())
    }

    #[test]
    fn test_invalid_url() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let record = HistoryRecord { id: SyncGuid("aaaaaaaaaaaa".to_string()),
                                     title: "title".into(),
                                     hist_uri: "invalid".into(),
                                     sortindex: 0,
                                     ttl: 100,
                                     visits: vec![]};

        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Invalid(_) => true,
            _ => false
        });
        Ok(())
    }

    #[test]
    fn test_new() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let visits = vec![HistoryRecordVisit {date: SystemTime::now().into(),
                                              transition: 1}];
        let record = HistoryRecord { id: SyncGuid("aaaaaaaaaaaa".to_string()),
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     ttl: 100,
                                     visits};

        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Apply(None, _, _, _) => true,
            _ => false,
        });
        Ok(())
    }

    #[test]
    fn test_dupe_visit_same_guid() -> Result<()> {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(now.into()));
        apply_observation(&mut conn, obs).expect("should apply");
        // should be New with a change counter.
        assert_eq!(get_sync(&conn, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&conn, &url);

        // try and add it remotely.
        let visits = vec![HistoryRecordVisit {date: now.into(), transition: 1}];
        let record = HistoryRecord { id: guid,
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     ttl: 100,
                                     visits };
        // We should have reconciled it.
        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Reconciled => true,
            _ => false,
        });
        Ok(())
    }

    #[test]
    fn test_dupe_visit_different_guid() {
        let _ = env_logger::try_init();
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(now.into()));
        apply_observation(&mut conn, obs).expect("should apply");

        let old_guid = get_existing_guid(&conn, &url);

        // try and add an incoming record with the same URL but different guid.
        let new_guid = random_guid().expect("according to logins-sql, this is fine :)");
        let visits = vec![HistoryRecordVisit {date: now.into(), transition: 1}];
        let record = HistoryRecord { id: new_guid.into(),
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     ttl: 100,
                                     visits };
        // Even though there are no visits we should record that it will be
        // applied with the guid change.
        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Apply(Some(got_old_guid), _, _, _) => {
                assert_eq!(got_old_guid, old_guid);
                true
            },
            _ => false,
        });
    }

    #[test]
    fn test_apply_plan_incoming_new() -> Result<()> {
        let _ = env_logger::try_init();
        let now: Timestamp = SystemTime::now().into();
        let json = json!({
            "id": "aaaaaaaaaaaa",
            "title": "title",
            "histUri": "http://example.com",
            "sortindex": 0,
            "ttl": 100,
            "visits": [ {"date": now, "type": 1}]
        });
        let mut result = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        result.changes.push((payload, ServerTimestamp(0f64)));

        let db = PlacesDb::open_in_memory(None)?;
        let outgoing = apply_plan(&db, result)?;

        // should have applied it locally.
        let (page, visits) = fetch_visits(&db, &Url::parse("http://example.com").unwrap(), 2)?
                             .expect("page exists");
        assert_eq!(page.title, "title");
        assert_eq!(visits.len(), 1);
        let visit = visits.into_iter().next().unwrap();
        assert_eq!(visit.visit_date, now);

        // page should have frecency (going through a public api to get this is a pain)
        // XXX - FIXME - searching for "title" here fails to find a result?
        // But above, we've checked title is in the record.
        let found = search_frecent(&db, SearchParams{ search_string: "http://example.com".into(), limit: 2 })?;
        assert_eq!(found.len(), 1);
        let result = found.into_iter().next().unwrap();
        assert!(result.frecency > 0, "should have frecency");

        // and nothing outgoing.
        assert_eq!(outgoing.changes.len(), 0);
        Ok(())
    }

    #[test]
    fn test_apply_plan_outgoing_new() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://example.com")?;
        let now = SystemTime::now();
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(now.into()));
        apply_observation(&mut db, obs)?;

        let incoming = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let outgoing = apply_plan(&db, incoming)?;

        assert_eq!(outgoing.changes.len(), 1);
        Ok(())
    }

    #[test]
    fn test_simple_visit_reconciliation() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let ts: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();
        let url = Url::parse("https://example.com")?;

        // First add a local visit with the timestamp.
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(ts));
        apply_observation(&mut db, obs)?;
        // Sync status should be "new" and have a change recorded.
        assert_eq!(get_sync(&db, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&db, &url);

        // and an incoming record with the same timestamp
        let json = json!({
            "id": guid,
            "title": "title",
            "histUri": url.as_str(),
            "sortindex": 0,
            "ttl": 100,
            "visits": [ {"date": ts, "type": 1}]
        });

        let mut incoming = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        incoming.changes.push((payload, ServerTimestamp(0f64)));

        apply_plan(&db, incoming)?;

        // should still have only 1 visit and it should still be local.
        let (_page, visits) = fetch_visits(&db, &url, 2)?.expect("page exists");
        assert_eq!(visits.len(), 1);
        assert_eq!(visits[0].is_local, true);
        // The item should have changed to Normal and have no change counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 0));
        Ok(())
    }

    #[test]
    fn test_simple_visit_incoming_and_outgoing() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let ts1: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();
        let ts2: Timestamp = SystemTime::now().into();
        let url = Url::parse("https://example.com")?;

        // First add a local visit with ts1.
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(ts1));
        apply_observation(&mut db, obs)?;

        let guid = get_existing_guid(&db, &url);

        // and an incoming record with ts2
        let json = json!({
            "id": guid,
            "title": "title",
            "histUri": url.as_str(),
            "sortindex": 0,
            "ttl": 100,
            "visits": [ {"date": ts2, "type": 1}]
        });

        let mut incoming = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        incoming.changes.push((payload, ServerTimestamp(0f64)));

        let outgoing = apply_plan(&db, incoming)?;

        // should now have both visits locally.
        let (_page, visits) = fetch_visits(&db, &url, 3)?.expect("page exists");
        assert_eq!(visits.len(), 2);

        // and the record should still be in outgoing due to our local change.
        assert_eq!(outgoing.changes.len(), 1);
        let out_maybe_record = HistorySyncRecord::from_payload(outgoing.changes[0].clone())?;
        assert_eq!(out_maybe_record.guid, guid);
        let record = out_maybe_record.record.expect("not a tombstone");
        assert_eq!(record.visits.len(), 2, "should have both visits outgoing");
        assert_eq!(record.visits[0].date, ts2, "most recent timestamp should be first");
        assert_eq!(record.visits[1].date, ts1, "both timestamps should appear");
        Ok(())
    }

    #[test]
    fn test_incoming_tombstone_local_new() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(SystemTime::now().into()));
        apply_observation(&mut db, obs)?;
        assert_eq!(get_sync(&db, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&db, &url);

        // and an incoming tombstone for that guid
        let json = json!({
            "id": guid,
            "deleted": true,
        });

        let mut incoming = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        incoming.changes.push((payload, ServerTimestamp(0f64)));

        let outgoing = apply_plan(&db, incoming)?;
        assert_eq!(outgoing.changes.len(), 0, "should be nothing outgoing");
        assert_eq!(get_tombstone_count(&db), 0, "should be no tombstones");
        Ok(())
    }

    #[test]
    fn test_incoming_tombstone_local_normal() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(SystemTime::now().into()));
        apply_observation(&mut db, obs)?;
        let guid = get_existing_guid(&db, &url);

        // Set the status to normal
        apply_plan(&db,
                   IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64)))?;
        // It should have changed to normal but still have the initial counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 1));

        // and an incoming tombstone for that guid
        let json = json!({
            "id": guid,
            "deleted": true,
        });

        let mut incoming = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        incoming.changes.push((payload, ServerTimestamp(0f64)));

        let outgoing = apply_plan(&db, incoming)?;
        assert_eq!(outgoing.changes.len(), 0, "should be nothing outgoing");
        Ok(())
    }

    #[test]
    fn test_outgoing_tombstone() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(SystemTime::now().into()));
        apply_observation(&mut db, obs)?;
        let guid = get_existing_guid(&db, &url);

        // Set the status to normal
        apply_plan(&db, IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64)))?;
        // It should have changed to normal but still have the initial counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 1));

        // Delete it.
        db.execute_named_cached(
            "DELETE FROM moz_places WHERE guid = :guid",
            &[(":guid", &guid)])?;

        // should be a local tombstone.
        assert_eq!(get_tombstone_count(&db), 1);

        let outgoing = apply_plan(&db, IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64)))?;
        assert_eq!(outgoing.changes.len(), 1, "tombstone should be uploaded");
        finish_plan(&db)?;
        // tombstone should be removed.
        assert_eq!(get_tombstone_count(&db), 0);

        Ok(())
    }
}
