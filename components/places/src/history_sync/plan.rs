/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::record::{HistoryRecord, HistoryRecordVisit};
use super::{MAX_OUTGOING_PLACES, MAX_VISITS};
use crate::api::history::can_add_url;
use crate::db::PlacesDb;
use crate::error::*;
use crate::storage::{
    delete_pending_temp_tables,
    history::history_sync::{
        apply_synced_deletion, apply_synced_reconciliation, apply_synced_visits, fetch_outgoing,
        fetch_visits, finish_outgoing, FetchedVisit, FetchedVisitPage,
    },
};
use crate::types::{UnknownFields, VisitType};
use interrupt_support::Interruptee;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use sync15::bso::{IncomingBso, IncomingKind, OutgoingBso};
use sync15::telemetry;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

/// Clamps a history visit date between the current date and the earliest
/// sensible date.
fn clamp_visit_date(visit_date: Timestamp) -> std::result::Result<Timestamp, ()> {
    let now = Timestamp::now();
    if visit_date > now {
        return Ok(now);
    }
    if visit_date < Timestamp::EARLIEST {
        return Err(());
    }
    Ok(visit_date)
}

/// This is the action we will take *locally* for each incoming record.
/// For example, IncomingPlan::Delete means we will be deleting a local record
/// and not that we will be uploading a tombstone or deleting the record itself.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum IncomingPlan {
    /// An entry we just want to ignore - either due to the URL etc, or because no changes.
    Skip,
    /// Something's wrong with this entry.
    Invalid(Error),
    /// The entry appears sane, but there was some error.
    Failed(Error),
    /// We should locally delete this.
    Delete,
    /// We should apply this.
    Apply {
        url: Url,
        new_title: Option<String>,
        visits: Vec<HistoryRecordVisit>,
        unknown_fields: UnknownFields,
    },
    /// Entry exists locally and it's the same as the incoming record. This is
    /// subtly different from Skip as we may still need to write metadata to
    /// the local DB for reconciled items.
    Reconciled,
}

fn plan_incoming_record(conn: &PlacesDb, record: HistoryRecord, max_visits: usize) -> IncomingPlan {
    let url = match Url::parse(&record.hist_uri) {
        Ok(u) => u,
        Err(e) => return IncomingPlan::Invalid(e.into()),
    };

    if !record.id.is_valid_for_places() {
        return IncomingPlan::Invalid(InvalidPlaceInfo::InvalidGuid.into());
    }

    match can_add_url(&url) {
        Ok(can) => {
            if !can {
                return IncomingPlan::Skip;
            }
        }
        Err(e) => return IncomingPlan::Failed(e),
    }
    // Let's get what we know about it, if anything - last 20, like desktop?
    let visit_tuple = match fetch_visits(conn, &url, max_visits) {
        Ok(v) => v,
        Err(e) => return IncomingPlan::Failed(e),
    };

    // This all seems more messy than it should be - struggling to find the
    // correct signature for fetch_visits.
    // An improvement might be to do this via a temp table so we can dedupe
    // and apply in one operation rather than the fetch, rust-merge and update
    // we are doing here.
    let (existing_page, existing_visits): (Option<FetchedVisitPage>, Vec<FetchedVisit>) =
        match visit_tuple {
            None => (None, Vec::new()),
            Some((p, v)) => (Some(p), v),
        };

    let guid_changed = match existing_page {
        Some(p) => p.guid != record.id,
        None => false,
    };

    let mut cur_visit_map: HashSet<(VisitType, Timestamp)> =
        HashSet::with_capacity(existing_visits.len());
    for visit in &existing_visits {
        // it should be impossible for us to have invalid visits locally, but...
        let transition = match visit.visit_type {
            Some(t) => t,
            None => continue,
        };
        match clamp_visit_date(visit.visit_date) {
            Ok(date_use) => {
                cur_visit_map.insert((transition, date_use));
            }
            Err(_) => {
                warn!("Ignored visit before 1993-01-23");
            }
        }
    }
    // If we already have MAX_RECORDS visits, then we will ignore incoming
    // visits older than that, to avoid adding dupes of earlier visits.
    // (Not really clear why 20 is magic, but what's good enough for desktop
    // is good enough for us at this stage.)
    // We should also consider pushing this deduping down into storage, where
    // it can possibly do a better job directly in SQL or similar.
    let earliest_allowed: SystemTime = if existing_visits.len() == max_visits {
        existing_visits[existing_visits.len() - 1].visit_date.into()
    } else {
        UNIX_EPOCH
    };

    // work out which of the incoming visits we should apply.
    let mut to_apply = Vec::with_capacity(record.visits.len());
    for incoming_visit in record.visits {
        let transition = match VisitType::from_primitive(incoming_visit.transition) {
            Some(v) => v,
            None => continue,
        };
        match clamp_visit_date(incoming_visit.date.into()) {
            Ok(timestamp) => {
                if earliest_allowed > timestamp.into() {
                    continue;
                }
                // If the entry isn't in our map we should add it.
                let key = (transition, timestamp);
                if !cur_visit_map.contains(&key) {
                    to_apply.push(HistoryRecordVisit {
                        date: timestamp.into(),
                        transition: transition as u8,
                        unknown_fields: incoming_visit.unknown_fields,
                    });
                    cur_visit_map.insert(key);
                }
            }
            Err(()) => {
                warn!("Ignored visit before 1993-01-23");
            }
        }
    }
    // Now we need to check the other attributes.
    // Check if we should update title? For now, assume yes. It appears
    // as though desktop always updates it.
    if guid_changed || !to_apply.is_empty() {
        let new_title = Some(record.title);
        IncomingPlan::Apply {
            url,
            new_title,
            visits: to_apply,
            unknown_fields: record.unknown_fields,
        }
    } else {
        IncomingPlan::Reconciled
    }
}

pub fn apply_plan(
    db: &PlacesDb,
    inbound: Vec<IncomingBso>,
    telem: &mut telemetry::EngineIncoming,
    interruptee: &impl Interruptee,
) -> Result<()> {
    // for a first-cut, let's do this in the most naive way possible...
    let mut plans: Vec<(SyncGuid, IncomingPlan)> = Vec::with_capacity(inbound.len());
    for incoming in inbound {
        interruptee.err_if_interrupted()?;
        let content = incoming.into_content::<HistoryRecord>();
        let plan = match content.kind {
            IncomingKind::Tombstone => IncomingPlan::Delete,
            IncomingKind::Content(record) => plan_incoming_record(db, record, MAX_VISITS),
            IncomingKind::Malformed => {
                // We could push IncomingPlan::Invalid here, but the code before the IncomingKind
                // refactor didn't know what `id` to use, so skipped it - so we do too.
                warn!(
                    "Error deserializing incoming record: {}",
                    content.envelope.id
                );
                telem.failed(1);
                continue;
            }
        };
        plans.push((content.envelope.id.clone(), plan));
    }

    let mut tx = db.begin_transaction()?;

    for (guid, plan) in plans {
        interruptee.err_if_interrupted()?;
        match &plan {
            IncomingPlan::Skip => {
                trace!("incoming: skipping item {:?}", guid);
                // XXX - should we `telem.reconciled(1);` here?
            }
            IncomingPlan::Invalid(err) => {
                warn!(
                    "incoming: record {:?} skipped because it is invalid: {}",
                    guid, err
                );
                telem.failed(1);
            }
            IncomingPlan::Failed(err) => {
                error_support::report_error!(
                    "places-failed-to-apply",
                    "incoming: record {:?} failed to apply: {}",
                    guid,
                    err
                );
                telem.failed(1);
            }
            IncomingPlan::Delete => {
                trace!("incoming: deleting {:?}", guid);
                apply_synced_deletion(db, &guid)?;
                telem.applied(1);
            }
            IncomingPlan::Apply {
                url,
                new_title,
                visits,
                unknown_fields,
            } => {
                trace!(
                    "incoming: will apply {guid:?}: url={url:?}, title={new_title:?}, to_add={visits:?}, unknown_fields={unknown_fields:?}"
                );
                apply_synced_visits(db, &guid, url, new_title, visits, unknown_fields)?;
                telem.applied(1);
            }
            IncomingPlan::Reconciled => {
                telem.reconciled(1);
                trace!("incoming: reconciled {:?}", guid);
                apply_synced_reconciliation(db, &guid)?;
            }
        };
        if tx.should_commit() {
            // Trigger frecency and origin updates before committing the
            // transaction, so that our origins table is consistent even
            // if we're interrupted.
            delete_pending_temp_tables(db)?;
        }
        tx.maybe_commit()?;
    }
    // ...And commit the final chunk of plans, making sure we trigger
    // frecency and origin updates.
    delete_pending_temp_tables(db)?;
    tx.commit()?;
    info!("incoming: {}", serde_json::to_string(&telem).unwrap());
    Ok(())
}

pub fn get_planned_outgoing(db: &PlacesDb) -> Result<Vec<OutgoingBso>> {
    // It might make sense for fetch_outgoing to manage its own
    // begin_transaction - even though doesn't seem a large bottleneck
    // at this time, the fact we hold a single transaction for the entire call
    // really is used only for performance, so it's certainly a candidate.
    let tx = db.begin_transaction()?;
    let outgoing = fetch_outgoing(db, MAX_OUTGOING_PLACES, MAX_VISITS)?;
    tx.commit()?;
    Ok(outgoing)
}

pub fn finish_plan(db: &PlacesDb) -> Result<()> {
    let tx = db.begin_transaction()?;
    finish_outgoing(db)?;
    trace!("Committing final sync plan");
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::matcher::{search_frecent, SearchParams};
    use crate::api::places_api::ConnectionType;
    use crate::db::PlacesDb;
    use crate::history_sync::ServerVisitTimestamp;
    use crate::observation::VisitObservation;
    use crate::storage::history::history_sync::fetch_visits;
    use crate::storage::history::{apply_observation, delete_visits_for, url_to_guid};
    use crate::types::SyncStatus;
    use interrupt_support::NeverInterrupts;
    use serde_json::json;
    use sql_support::ConnExt;
    use std::time::Duration;
    use sync15::bso::IncomingBso;
    use types::Timestamp;
    use url::Url;

    fn get_existing_guid(conn: &PlacesDb, url: &Url) -> SyncGuid {
        url_to_guid(conn, url)
            .expect("should have worked")
            .expect("should have got a value")
    }

    fn get_tombstone_count(conn: &PlacesDb) -> u32 {
        let result: Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_places_tombstones;",
            [],
            |row| Ok(row.get::<_, u32>(0)?),
            true,
        );
        result
            .expect("should have worked")
            .expect("should have got a value")
    }

    fn get_sync(conn: &PlacesDb, url: &Url) -> (SyncStatus, u32) {
        let guid_result: Result<Option<(SyncStatus, u32)>> = conn.try_query_row(
            "SELECT sync_status, sync_change_counter
                     FROM moz_places
                     WHERE url = :url;",
            &[(":url", &String::from(url.clone()))],
            |row| {
                Ok((
                    SyncStatus::from_u8(row.get::<_, u8>(0)?),
                    row.get::<_, u32>(1)?,
                ))
            },
            true,
        );
        guid_result
            .expect("should have worked")
            .expect("should have got values")
    }

    fn apply_and_get_outgoing(db: &PlacesDb, incoming: Vec<IncomingBso>) -> Vec<OutgoingBso> {
        apply_plan(
            db,
            incoming,
            &mut telemetry::EngineIncoming::new(),
            &NeverInterrupts,
        )
        .expect("should apply");
        get_planned_outgoing(db).expect("should get outgoing")
    }

    #[test]
    fn test_invalid_guid() -> Result<()> {
        error_support::init_for_tests();
        let conn = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let record = HistoryRecord {
            id: "foo".into(),
            title: "title".into(),
            hist_uri: "http://example.com".into(),
            visits: vec![],
            unknown_fields: UnknownFields::new(),
        };

        assert!(matches!(
            plan_incoming_record(&conn, record, 10),
            IncomingPlan::Invalid(_)
        ));
        Ok(())
    }

    #[test]
    fn test_invalid_url() -> Result<()> {
        error_support::init_for_tests();
        let conn = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let record = HistoryRecord {
            id: "aaaaaaaaaaaa".into(),
            title: "title".into(),
            hist_uri: "invalid".into(),
            visits: vec![],
            unknown_fields: UnknownFields::new(),
        };

        assert!(matches!(
            plan_incoming_record(&conn, record, 10),
            IncomingPlan::Invalid(_)
        ));
        Ok(())
    }

    #[test]
    fn test_new() -> Result<()> {
        error_support::init_for_tests();
        let conn = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let visits = vec![HistoryRecordVisit {
            date: SystemTime::now().into(),
            transition: 1,
            unknown_fields: UnknownFields::new(),
        }];
        let record = HistoryRecord {
            id: "aaaaaaaaaaaa".into(),
            title: "title".into(),
            hist_uri: "https://example.com".into(),
            visits,
            unknown_fields: UnknownFields::new(),
        };

        assert!(matches!(
            plan_incoming_record(&conn, record, 10),
            IncomingPlan::Apply { .. }
        ));
        Ok(())
    }

    #[test]
    fn test_plan_dupe_visit_same_guid() {
        error_support::init_for_tests();
        let conn = PlacesDb::open_in_memory(ConnectionType::Sync).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(now.into()));
        apply_observation(&conn, obs).expect("should apply");
        // should be New with a change counter.
        assert_eq!(get_sync(&conn, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&conn, &url);

        // try and add it remotely.
        let visits = vec![HistoryRecordVisit {
            date: now.into(),
            transition: 1,
            unknown_fields: UnknownFields::new(),
        }];
        let record = HistoryRecord {
            id: guid,
            title: "title".into(),
            hist_uri: "https://example.com".into(),
            visits,
            unknown_fields: UnknownFields::new(),
        };
        // We should have reconciled it.
        assert!(matches!(
            plan_incoming_record(&conn, record, 10),
            IncomingPlan::Reconciled
        ));
    }

    #[test]
    fn test_plan_dupe_visit_different_guid_no_visits() {
        error_support::init_for_tests();
        let conn = PlacesDb::open_in_memory(ConnectionType::Sync).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(now.into()));
        apply_observation(&conn, obs).expect("should apply");

        assert_eq!(get_sync(&conn, &url), (SyncStatus::New, 1));

        // try and add an incoming record with the same URL but different guid.
        let record = HistoryRecord {
            id: SyncGuid::random(),
            title: "title".into(),
            hist_uri: "https://example.com".into(),
            visits: vec![],
            unknown_fields: UnknownFields::new(),
        };
        // Even though there are no visits we should record that it will be
        // applied with the guid change.
        assert!(matches!(
            plan_incoming_record(&conn, record, 10),
            IncomingPlan::Apply { .. }
        ));
    }

    // These "dupe" tests all do the full application of the plan and checks
    // the end state of the db.
    #[test]
    fn test_apply_dupe_no_local_visits() -> Result<()> {
        // There's a chance the server ends up with different records but
        // which reference the same URL.
        // This is testing the case when there are no local visits to that URL.
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let guid1 = SyncGuid::random();
        let ts1: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();

        let guid2 = SyncGuid::random();
        let ts2: Timestamp = SystemTime::now().into();
        let url = Url::parse("https://example.com")?;

        // 2 incoming records with the same URL.
        let incoming = vec![
            IncomingBso::from_test_content(json!({
                "id": guid1,
                "title": "title",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts1), "type": 1}]
            })),
            IncomingBso::from_test_content(json!({
                "id": guid2,
                "title": "title",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts2), "type": 1}]
            })),
            IncomingBso::from_test_content(json!({
                "id": guid2,
                "title": "title2",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts2), "type": 1}]
            })),
        ];

        let outgoing = apply_and_get_outgoing(&db, incoming);
        assert_eq!(
            outgoing.len(),
            1,
            "should have guid1 as outgoing with both visits."
        );
        assert_eq!(outgoing[0].envelope.id, guid1);

        // should have 1 URL with both visits locally.
        let (page, visits) = fetch_visits(&db, &url, 3)?.expect("page exists");
        assert_eq!(
            page.guid, guid1,
            "page should have the guid from the first record"
        );
        assert_eq!(
            page.title, "title2",
            "page should have the title from the second record"
        );
        assert_eq!(visits.len(), 2, "page should have 2 visits");

        Ok(())
    }

    #[test]
    fn test_apply_dupe_local_unsynced_visits() -> Result<()> {
        // There's a chance the server ends up with different records but
        // which reference the same URL.
        // This is testing the case when there are a local visits to that URL,
        // but they are yet to be synced - the local guid should change and
        // all visits should be applied.
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;

        let guid1 = SyncGuid::random();
        let ts1: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();

        let guid2 = SyncGuid::random();
        let ts2: Timestamp = SystemTime::now().into();
        let url = Url::parse("https://example.com")?;

        let ts_local: Timestamp = (SystemTime::now() - Duration::new(10, 0)).into();
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(ts_local));
        apply_observation(&db, obs)?;

        // 2 incoming records with the same URL.
        let incoming = vec![
            IncomingBso::from_test_content(json!({
                "id": guid1,
                "title": "title",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts1), "type": 1}]
            })),
            IncomingBso::from_test_content(json!({
                "id": guid2,
                "title": "title",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts2), "type": 1}]
            })),
        ];

        let outgoing = apply_and_get_outgoing(&db, incoming);
        assert_eq!(outgoing.len(), 1, "should have guid1 as outgoing");
        assert_eq!(outgoing[0].envelope.id, guid1);

        // should have 1 URL with all visits locally, but with the first incoming guid.
        let (page, visits) = fetch_visits(&db, &url, 3)?.expect("page exists");
        assert_eq!(page.guid, guid1, "should have the expected guid");
        assert_eq!(visits.len(), 3, "should have all visits");

        Ok(())
    }

    #[test]
    fn test_apply_dupe_local_synced_visits() -> Result<()> {
        // There's a chance the server ends up with different records but
        // which reference the same URL.
        // This is testing the case when there are a local visits to that URL,
        // and they have been synced - the existing guid should not change,
        // although all visits should still be applied.
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;

        let guid1 = SyncGuid::random();
        let ts1: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();

        let guid2 = SyncGuid::random();
        let ts2: Timestamp = SystemTime::now().into();
        let url = Url::parse("https://example.com")?;

        let ts_local: Timestamp = (SystemTime::now() - Duration::new(10, 0)).into();
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(ts_local));
        apply_observation(&db, obs)?;

        // 2 incoming records with the same URL.
        let incoming = vec![
            IncomingBso::from_test_content(json!({
                "id": guid1,
                "title": "title",
                "histUri": url.as_str(),
                "visits": [ {"date": ServerVisitTimestamp::from(ts1), "type": 1}]
            })),
            IncomingBso::from_test_content(json!({
                "id": guid2,
                "title": "title",
                "histUri": url.as_str(),
                "sortindex": 0,
                "ttl": 100,
                "visits": [ {"date": ServerVisitTimestamp::from(ts2), "type": 1}]
            })),
        ];

        let outgoing = apply_and_get_outgoing(&db, incoming);
        assert_eq!(
            outgoing.len(),
            1,
            "should have guid1 as outgoing with both visits."
        );

        // should have 1 URL with all visits locally, but with the first incoming guid.
        let (page, visits) = fetch_visits(&db, &url, 3)?.expect("page exists");
        assert_eq!(page.guid, guid1, "should have the expected guid");
        assert_eq!(visits.len(), 3, "should have all visits");

        Ok(())
    }

    #[test]
    fn test_apply_plan_incoming_invalid_timestamp() -> Result<()> {
        error_support::init_for_tests();
        let json = json!({
            "id": "aaaaaaaaaaaa",
            "title": "title",
            "histUri": "http://example.com",
            "visits": [ {"date": 15_423_493_234_840_000_000u64, "type": 1}]
        });
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);
        assert_eq!(outgoing.len(), 0, "nothing outgoing");

        let now: Timestamp = SystemTime::now().into();
        let (_page, visits) =
            fetch_visits(&db, &Url::parse("http://example.com").unwrap(), 2)?.expect("page exists");
        assert_eq!(visits.len(), 1);
        assert!(
            visits[0].visit_date <= now,
            "should have clamped the timestamp"
        );
        Ok(())
    }

    #[test]
    fn test_apply_plan_incoming_invalid_negative_timestamp() -> Result<()> {
        error_support::init_for_tests();
        let json = json!({
            "id": "aaaaaaaaaaaa",
            "title": "title",
            "histUri": "http://example.com",
            "visits": [ {"date": -123, "type": 1}]
        });
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);
        assert_eq!(outgoing.len(), 0, "should skip the invalid entry");
        Ok(())
    }

    #[test]
    fn test_apply_plan_incoming_invalid_visit_type() -> Result<()> {
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let visits = vec![HistoryRecordVisit {
            date: SystemTime::now().into(),
            transition: 99,
            unknown_fields: UnknownFields::new(),
        }];
        let record = HistoryRecord {
            id: "aaaaaaaaaaaa".into(),
            title: "title".into(),
            hist_uri: "http://example.com".into(),
            visits,
            unknown_fields: UnknownFields::new(),
        };
        let plan = plan_incoming_record(&db, record, 10);
        // We expect "Reconciled" because after skipping the invalid visit
        // we found nothing to apply.
        assert!(matches!(plan, IncomingPlan::Reconciled));
        Ok(())
    }

    #[test]
    fn test_apply_plan_incoming_new() -> Result<()> {
        error_support::init_for_tests();
        let now: Timestamp = SystemTime::now().into();
        let json = json!({
            "id": "aaaaaaaaaaaa",
            "title": "title",
            "histUri": "http://example.com",
            "visits": [ {"date": ServerVisitTimestamp::from(now), "type": 1}]
        });
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);

        // should have applied it locally.
        let (page, visits) =
            fetch_visits(&db, &Url::parse("http://example.com").unwrap(), 2)?.expect("page exists");
        assert_eq!(page.title, "title");
        assert_eq!(visits.len(), 1);
        let visit = visits.into_iter().next().unwrap();
        assert_eq!(visit.visit_date, now);

        // page should have frecency (going through a public api to get this is a pain)
        // XXX - FIXME - searching for "title" here fails to find a result?
        // But above, we've checked title is in the record.
        let found = search_frecent(
            &db,
            SearchParams {
                search_string: "http://example.com".into(),
                limit: 2,
            },
        )?;
        assert_eq!(found.len(), 1);
        let result = found.into_iter().next().unwrap();
        assert!(result.frecency > 0, "should have frecency");

        // and nothing outgoing.
        assert_eq!(outgoing.len(), 0);
        Ok(())
    }

    #[test]
    fn test_apply_plan_outgoing_new() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let url = Url::parse("https://example.com")?;
        let now = SystemTime::now();
        let obs = VisitObservation::new(url)
            .with_visit_type(VisitType::Link)
            .with_at(Some(now.into()));
        apply_observation(&db, obs)?;

        let outgoing = apply_and_get_outgoing(&db, vec![]);

        assert_eq!(outgoing.len(), 1);
        Ok(())
    }

    #[test]
    fn test_simple_visit_reconciliation() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let ts: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();
        let url = Url::parse("https://example.com")?;

        // First add a local visit with the timestamp.
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(ts));
        apply_observation(&db, obs)?;
        // Sync status should be "new" and have a change recorded.
        assert_eq!(get_sync(&db, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&db, &url);

        // and an incoming record with the same timestamp
        let json = json!({
            "id": guid,
            "title": "title",
            "histUri": url.as_str(),
            "visits": [ {"date": ServerVisitTimestamp::from(ts), "type": 1}]
        });

        apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);

        // should still have only 1 visit and it should still be local.
        let (_page, visits) = fetch_visits(&db, &url, 2)?.expect("page exists");
        assert_eq!(visits.len(), 1);
        assert!(visits[0].is_local);
        // The item should have changed to Normal and have no change counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 0));
        Ok(())
    }

    #[test]
    fn test_simple_visit_incoming_and_outgoing() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let ts1: Timestamp = (SystemTime::now() - Duration::new(5, 0)).into();
        let ts2: Timestamp = SystemTime::now().into();
        let url = Url::parse("https://example.com")?;

        // First add a local visit with ts1.
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(ts1));
        apply_observation(&db, obs)?;

        let guid = get_existing_guid(&db, &url);

        // and an incoming record with ts2
        let json = json!({
            "id": guid,
            "title": "title",
            "histUri": url.as_str(),
            "visits": [ {"date": ServerVisitTimestamp::from(ts2), "type": 1}]
        });

        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);

        // should now have both visits locally.
        let (_page, visits) = fetch_visits(&db, &url, 3)?.expect("page exists");
        assert_eq!(visits.len(), 2);

        // and the record should still be in outgoing due to our local change.
        assert_eq!(outgoing.len(), 1);
        let record = outgoing[0].to_test_incoming_t::<HistoryRecord>();
        assert_eq!(record.id, guid);
        assert_eq!(record.visits.len(), 2, "should have both visits outgoing");
        assert_eq!(
            record.visits[0].date,
            ts2.into(),
            "most recent timestamp should be first"
        );
        assert_eq!(
            record.visits[1].date,
            ts1.into(),
            "both timestamps should appear"
        );
        Ok(())
    }

    #[test]
    fn test_incoming_tombstone_local_new() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(SystemTime::now().into()));
        apply_observation(&db, obs)?;
        assert_eq!(get_sync(&db, &url), (SyncStatus::New, 1));

        let guid = get_existing_guid(&db, &url);

        // and an incoming tombstone for that guid
        let json = json!({
            "id": guid,
            "deleted": true,
        });
        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);
        assert_eq!(outgoing.len(), 0, "should be nothing outgoing");
        assert_eq!(get_tombstone_count(&db), 0, "should be no tombstones");
        Ok(())
    }

    #[test]
    fn test_incoming_tombstone_local_normal() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(SystemTime::now().into()));
        apply_observation(&db, obs)?;
        let guid = get_existing_guid(&db, &url);

        // Set the status to normal
        apply_and_get_outgoing(&db, vec![]);
        // It should have changed to normal but still have the initial counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 1));

        // and an incoming tombstone for that guid
        let json = json!({
            "id": guid,
            "deleted": true,
        });

        let outgoing = apply_and_get_outgoing(&db, vec![IncomingBso::from_test_content(json)]);
        assert_eq!(outgoing.len(), 0, "should be nothing outgoing");
        Ok(())
    }

    #[test]
    fn test_outgoing_tombstone() -> Result<()> {
        error_support::init_for_tests();
        let db = PlacesDb::open_in_memory(ConnectionType::Sync)?;
        let url = Url::parse("https://example.com")?;
        let obs = VisitObservation::new(url.clone())
            .with_visit_type(VisitType::Link)
            .with_at(Some(SystemTime::now().into()));
        apply_observation(&db, obs)?;
        let guid = get_existing_guid(&db, &url);

        // Set the status to normal
        apply_and_get_outgoing(&db, vec![]);
        // It should have changed to normal but still have the initial counter.
        assert_eq!(get_sync(&db, &url), (SyncStatus::Normal, 1));

        // Delete it.
        delete_visits_for(&db, &guid)?;

        // should be a local tombstone.
        assert_eq!(get_tombstone_count(&db), 1);

        let outgoing = apply_and_get_outgoing(&db, vec![]);
        assert_eq!(outgoing.len(), 1, "tombstone should be uploaded");
        finish_plan(&db)?;
        // tombstone should be removed.
        assert_eq!(get_tombstone_count(&db), 0);

        Ok(())
    }

    #[test]
    fn test_clamp_visit_date() {
        let ts = Timestamp::from(727_747_199_999);
        assert!(clamp_visit_date(ts).is_err());

        let ts = Timestamp::now();
        assert_eq!(clamp_visit_date(ts), Ok(ts));
    }
}
