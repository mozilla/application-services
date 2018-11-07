/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection};
use url::{Url};

use super::{MAX_VISITS};
use super::record::{HistoryRecord, HistoryRecordVisit, HistorySyncRecord};
use types::{SyncGuid, Timestamp, VisitTransition};
use storage::history_sync::{
    apply_synced_deletion,
    apply_synced_visits,
    fetch_visits,
    FetchedVisit,
    FetchedVisitPage
};
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
    Delete(), // We should delete this.
    // We should apply this. If SyncGuid is Some, then it is the existing guid
    // for the same URL, and if that doesn't match the incoming record, we
    // should change the existing item to the incoming one and upload a tombstone.
    Apply(Option<SyncGuid>, Url, Option<String>, Vec<HistoryRecordVisit>),
}


fn plan_incoming_record(conn: &Connection, record: HistoryRecord, max_visits: usize) -> IncomingPlan {
    let url = match Url::parse(&record.hist_uri) {
        Ok(u) => u,
        Err(e) => return IncomingPlan::Invalid(e.into()),
    };
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
    let mut cur_visit_map: HashSet<(VisitTransition, Timestamp)> = HashSet::new();
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
        // XXX - check if we should update title? For now, assume yes.
        let new_title = Some(record.title);
        IncomingPlan::Apply(old_guid, url.clone(), new_title, to_apply)
    } else {
        IncomingPlan::Skip
    }
}

pub fn apply_plan(conn: &Connection, inbound: IncomingChangeset) -> Result<OutgoingChangeset> {
    // for a first-cut, let's do this in the most naive way possible...
    let mut plans: Vec<(SyncGuid, IncomingPlan)> = Vec::with_capacity(inbound.changes.len());
    for incoming in inbound.changes.iter() {
        let item = HistorySyncRecord::from_payload(incoming.0.clone())?;
        let plan = match item.record {
            Some(record) => plan_incoming_record(conn, record, MAX_VISITS),
            None => IncomingPlan::Delete(),
        };
        let guid = item.guid.clone();
        plans.push((guid, plan));
    }

    let mut outgoing = OutgoingChangeset::new("history".into(), inbound.timestamp);
    for (guid, plan) in plans {
        match &plan {
            IncomingPlan::Skip => {
                trace!("skipping the item");
            },
            IncomingPlan::Invalid(err) => {
                warn!("record {:?} skipped because it is invalid: {}", guid, err);
            },
            IncomingPlan::Failed(err) => {
                error!("record {:?} failed to apply: {}", guid, err);
            },
            IncomingPlan::Delete() => {
                trace!("deleting {:?}", guid);
                apply_synced_deletion(&conn, &guid)?;
            },
            IncomingPlan::Apply(old_guid, url, title, visits_to_add) => {
                trace!("will apply {:?}: url={:?}, title={:?}, to_add={}",
                      guid, url, title, visits_to_add.len());
                apply_synced_visits(&conn, &guid, &old_guid, &url, title, visits_to_add)?;
                if let Some(ref old_guid) = old_guid {
                    outgoing.changes.push(Payload::new_tombstone(old_guid.0.clone()));
                }
            },
        };
    }
    Ok(outgoing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::{apply_observation};
    use storage::history_sync::{fetch_visits};
    use observation::{VisitObservation};
    use api::matcher::{SearchParams, search_frecent};
    use types::{Timestamp};
    use db::PlacesDb;

    use sql_support::ConnExt;
    use sync15_adapter::util::{random_guid};

    use sync15_adapter::{IncomingChangeset, ServerTimestamp};
    use url::{Url};

    fn get_existing_guid(conn: &PlacesDb, url: &Url) -> SyncGuid {
        let guid_result: Result<Option<String>> = conn.try_query_row(
                    "SELECT guid from moz_places WHERE url = :url;",
                    &[(":url", &url.clone().into_string())],
                    |row| Ok(row.get::<_, String>(0).clone()), true);
        guid_result.expect("should have worked").expect("should have got a value").into()
    }

    #[test]
    fn test_invalid() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let record = HistoryRecord { id: SyncGuid("foo".to_string()),
                                     title: "title".into(),
                                     hist_uri: "invalid".into(),
                                     sortindex: 0,
                                     visits: vec![]};

        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Invalid(_) => true,
            _ => false
        });
    }

    #[test]
    fn test_new() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let visits = vec![HistoryRecordVisit {date: SystemTime::now().into(),
                                              transition: 1}];
        let record = HistoryRecord { id: SyncGuid("foo".to_string()),
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     visits};

        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Apply(None, _, _, _) => true,
            _ => false,
        });
    }

    #[test]
    fn test_dupe_visit_same_guid() {
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(now.into()));
        apply_observation(&mut conn, obs).expect("should apply");

        let guid = get_existing_guid(&conn, &url);

        // try and add it remotely.
        let visits = vec![HistoryRecordVisit {date: now.into(), transition: 1}];
        let record = HistoryRecord { id: guid,
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     visits };
        // We should skip it.
        assert!(match plan_incoming_record(&conn, record, 10) {
            IncomingPlan::Skip => true,
            _ => false,
        });
    }

    #[test]
    fn test_dupe_visit_different_guid() {
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
    fn test_apply_plan_incoming_new() {
        let now: Timestamp = SystemTime::now().into();
        let json = json!({
            "id": "foo",
            "title": "title",
            "histUri": "http://example.com",
            "sortindex": 0,
            "visits": [ {"date": now, "type": 1}]
        });
        let mut result = IncomingChangeset::new("history".to_string(), ServerTimestamp(0f64));
        let payload = Payload::from_json(json).unwrap();
        result.changes.push((payload, ServerTimestamp(0f64)));

        let db = PlacesDb::open_in_memory(None).expect("no memory db");
        let outgoing = apply_plan(&db, result).expect("should work");

        // should have applied it locally.
        let (page, visits) = fetch_visits(&db, &Url::parse("http://example.com").unwrap(), 2)
                             .expect("should work").expect("page exists");
        assert_eq!(page.title, "title");
        assert_eq!(visits.len(), 1);
        let visit = visits.into_iter().next().unwrap();
        assert_eq!(visit.visit_date, now);

        // page should have frecency (going through a public api to get this is a pain)
        // XXX - FIXME - searching for "title" here fails to find a result?
        // But above, we've checked title is in the record.
        let found = search_frecent(&db, SearchParams{ search_string: "http://example.com".into(), limit: 2 }
                                   ).expect("should have worked");
        assert_eq!(found.len(), 1);
        let result = found.into_iter().next().unwrap();
        assert!(result.frecency > 0, "should have frecency");

        // and nothing outgoing.
        assert_eq!(outgoing.changes.len(), 0);
    }
}
