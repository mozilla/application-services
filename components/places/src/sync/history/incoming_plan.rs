/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use error::*;
use rusqlite::{Connection};
use url::{Url};

use types::{SyncGuid, Timestamp, VisitTransition};
use super::record::{HistoryRecord, HistoryRecordVisit};
use super::super::super::api::history::{can_add_url};
use super::super::super::storage::{fetch_visits, FetchedVisit, FetchedVisitPage} ;

/// Clamps a history visit date between the current date and the earliest
/// sensible date.
fn clamp_visit_date(visit_date: Timestamp) -> Timestamp {
    let now = Timestamp::now();
    if visit_date > now {
        return visit_date;
    }
    if visit_date < *EARLIEST_BOOKMARK_TIMESTAMP {
        return *EARLIEST_BOOKMARK_TIMESTAMP;
    }
    return visit_date;
}

#[derive(Debug)]
pub enum IncomingPlan {
    Skip, // An entry we just want to ignore - either due to the URL etc, or because no changes.
    Invalid(SyncGuid, Error), // Something's wrong with this entry.
    Failed(SyncGuid, Error), // The entry appears sane, but there was some error.
    Delete(SyncGuid), // We should delete this.
    Apply(SyncGuid, Url, Option<String>, Vec<HistoryRecordVisit>),
}


pub fn plan_incoming_record(conn: &Connection, record: HistoryRecord) -> IncomingPlan {
    let url = match Url::parse(&record.hist_uri) {
        Ok(u) => u,
        Err(e) => return IncomingPlan::Invalid(record.id.clone(), e.into()),
    };
    match can_add_url(&url) {
        Ok(can) => if !can { return IncomingPlan::Skip },
        Err(e) => return IncomingPlan::Failed(record.id.clone(), e.into()),
    }
    // Let's get what we know about it, if anything - last 20, like desktop?
    const MAX_RECORDS: u32 = 20;
    let visit_tuple = match fetch_visits(conn, &url, MAX_RECORDS) {
        Ok(v) => v,
        Err(e) => return IncomingPlan::Failed(record.id.clone(), e.into()),
    };

    // This all seems more messy than it should be - struggling to find the
    // correct signature for fetch_visits
    let (existing_page, existing_visits): (Option<FetchedVisitPage>, Vec<FetchedVisit>) = match visit_tuple {
        None => (None, Vec::new()),
        Some((p, v)) => (Some(p), v),
    };

    let mut cur_visit_map: HashSet<(VisitTransition, Timestamp)> = HashSet::new();
    if let Some(p) = &existing_page {
        assert_eq!(p.guid, record.id); // XXX - we need to handle a change of guid.
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
    let earliest_allowed: SystemTime = if existing_visits.len() == MAX_RECORDS as usize {
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
    if to_apply.len() != 0 {
        // Now we need to check the other attributes.
        // XXX - check if we should update title? For now, assume yes.
        let new_title = Some(record.title);
        IncomingPlan::Apply(record.id, url.clone(), new_title, to_apply)
    } else {
        IncomingPlan::Skip
    }
}

lazy_static! {
    static ref EARLIEST_BOOKMARK_TIMESTAMP: Timestamp = Timestamp(0); // XXX - want Jan 23, 1993
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::super::storage::{apply_observation};
    use super::super::super::super::observation::{VisitObservation};

    use db::PlacesDb;

    #[test]
    fn test_invalid() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let record = HistoryRecord { id: SyncGuid("foo".to_string()),
                                     title: "title".into(),
                                     hist_uri: "invalid".into(),
                                     sortindex: 0,
                                     visits: vec![]};

        assert!(match plan_incoming_record(&conn, record) {
            IncomingPlan::Invalid(_, _) => true,
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

        assert!(match plan_incoming_record(&conn, record) {
            IncomingPlan::Apply(_, _, _, _) => true,
            _ => false,
        });
    }

    #[test]
    fn test_dupe_visit() {
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let now = SystemTime::now();
        let url = Url::parse("https://example.com").expect("is valid");
        // add it locally
        let obs = VisitObservation::new(url.clone())
                      .with_visit_type(VisitTransition::Link)
                      .with_at(Some(now.into()));
        apply_observation(&mut conn, obs).expect("should apply");
        // try and add it remotely.
        let visits = vec![HistoryRecordVisit {date: now.into(), transition: 1}];
        let record = HistoryRecord { id: SyncGuid("foo".to_string()),
                                     title: "title".into(),
                                     hist_uri: "https://example.com".into(),
                                     sortindex: 0,
                                     visits };
        // We should skip it.
        assert!(match plan_incoming_record(&conn, record) {
            IncomingPlan::Skip => true,
            _ => false,
        });
    }
}
