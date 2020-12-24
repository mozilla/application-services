/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::{AddressChanges, Record, RecordData};
use crate::error::*;
use interrupt_support::Interruptee;
use rusqlite::{named_params, types::ToSql, Connection};
use serde_json::{Map, Value};
use sql_support::ConnExt;
use sync15::Payload;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

pub fn stage_incoming(
    conn: &Connection,
    incoming_payloads: Vec<Payload>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let mut incoming_records = Vec::with_capacity(incoming_payloads.len());
    let mut incoming_tombstones = Vec::with_capacity(incoming_payloads.len());

    for payload in incoming_payloads {
        match payload.deleted {
            true => incoming_tombstones.push(payload.into_record::<Record>().unwrap()),
            false => incoming_records.push(payload.into_record::<Record>().unwrap()),
        };
    }
    save_incoming_records(conn, incoming_records, signal)?;
    save_incoming_tombstones(conn, incoming_tombstones, signal)?;
    Ok(())
}

fn save_incoming_records(
    conn: &Connection,
    incoming_records: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
    match incoming_records.is_empty() {
        true => Ok(()),
        false => {
            let chunk_size = 13;
            sql_support::each_sized_chunk(
                &incoming_records,
                sql_support::default_max_variable_number() / chunk_size,
                |chunk, _| -> Result<()> {
                    let sql = format!(
                        "INSERT OR REPLACE INTO temp.addresses_sync_staging (
                            guid,
                            given_name,
                            additional_name,
                            family_name,
                            organization,
                            street_address,
                            address_level3,
                            address_level2,
                            address_level1,
                            postal_code,
                            country,
                            tel,
                            email
                        ) VALUES {}",
                        sql_support::repeat_multi_values(chunk.len(), chunk_size)
                    );
                    let mut params = Vec::with_capacity(chunk.len() * chunk_size);
                    for record in chunk {
                        signal.err_if_interrupted()?;
                        params.push(&record.guid as &dyn ToSql);
                        params.push(&record.data.given_name);
                        params.push(&record.data.additional_name);
                        params.push(&record.data.family_name);
                        params.push(&record.data.organization);
                        params.push(&record.data.street_address);
                        params.push(&record.data.address_level3);
                        params.push(&record.data.address_level2);
                        params.push(&record.data.address_level1);
                        params.push(&record.data.postal_code);
                        params.push(&record.data.country);
                        params.push(&record.data.tel);
                        params.push(&record.data.email);
                    }
                    conn.execute(&sql, &params)?;
                    Ok(())
                },
            )
        }
    }
}

fn save_incoming_tombstones(
    conn: &Connection,
    incoming_tombstones: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
    match incoming_tombstones.is_empty() {
        true => Ok(()),
        false => {
            let chunk_size = 1;
            sql_support::each_sized_chunk(
                &incoming_tombstones,
                sql_support::default_max_variable_number() / chunk_size,
                |chunk, _| -> Result<()> {
                    let sql = format!(
                        "INSERT OR REPLACE INTO temp.addresses_tombstone_sync_staging (
                            guid
                        ) VALUES {}",
                        sql_support::repeat_multi_values(chunk.len(), chunk_size)
                    );
                    let mut params = Vec::with_capacity(chunk.len() * chunk_size);
                    for record in chunk {
                        signal.err_if_interrupted()?;
                        params.push(&record.guid as &dyn ToSql);
                    }
                    conn.execute(&sql, &params)?;
                    Ok(())
                },
            )
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum IncomingState {
    IncomingOnly {
        guid: String,
        incoming: RecordData,
    },
    IncomingTombstone {
        guid: String,
        local: RecordData,
        mirror: RecordData,
    },
    HasLocal {
        guid: String,
        incoming: RecordData,
        local: RecordData,
        mirror: Option<RecordData>,
    },
    // In desktop, if a local record with the remote's guid doesn't exist, an attempt is made
    // to find a local dupe of the remote record
    // (https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillSync.jsm#132).
    // `HasLocalDupe` is the state which represents when said dupe is found. This is logic
    // that may need to be updated in the future, but currently exists solely for the purpose of reaching
    // parity with desktop.
    HasLocalDupe {
        guid: String,
        incoming: RecordData,
        dupe_guid: String,
        dupe: RecordData,
        mirror: Option<RecordData>,
    },
    LocalTombstone {
        guid: String,
        incoming: RecordData,
    },
}

pub fn get_incoming(conn: &Connection) -> Result<Vec<(SyncGuid, IncomingState)>> {
    let mut incoming_states = get_incoming_tombstone_states(conn)?;
    let mut incoming_record_states = get_incoming_record_states(conn)?;
    incoming_states.append(&mut incoming_record_states);

    Ok(incoming_states)
}

#[allow(dead_code)]
fn get_incoming_tombstone_states(conn: &Connection) -> Result<Vec<(SyncGuid, IncomingState)>> {
    Ok(conn.conn().query_rows_and_then_named(
        "SELECT
            s.guid,
            l.given_name as l_given_name,
            m.given_name as m_given_name,
            l.additional_name as l_additional_name,
            m.additional_name as m_additional_name,
            l.family_name as l_family_name,
            m.family_name as m_family_name,
            l.organization as l_organization,
            m.organization as m_organization,
            l.street_address as l_street_address,
            m.street_address as m_street_address,
            l.address_level3 as l_address_level3,
            m.address_level3 as m_address_level3,
            l.address_level2 as l_address_level2,
            m.address_level2 as m_address_level2,
            l.address_level1 as l_address_level1,
            m.address_level1 as m_address_level1,
            l.postal_code as l_postal_code,
            m.postal_code as m_postal_code,
            l.country as l_country,
            m.country as m_country,
            l.tel as l_tel,
            m.tel as m_tel,
            l.email as l_email,
            m.email as m_email
        FROM temp.addresses_tombstone_sync_staging s
        JOIN addresses_mirror m ON s.guid = m.guid
        JOIN addresses_data l ON s.guid = l.guid",
        &[],
        |row| -> Result<(SyncGuid, IncomingState)> {
            let guid: SyncGuid = row.get("s_guid")?;
            let guid_str = guid.to_string();
            Ok((
                guid,
                IncomingState::IncomingTombstone {
                    guid: guid_str,
                    local: RecordData {
                        given_name: row.get("l_given_name")?,
                        additional_name: row.get("l_additional_name")?,
                        family_name: row.get("l_family_name")?,
                        organization: row.get("l_organization")?,
                        street_address: row.get("l_street_address")?,
                        address_level3: row.get("l_address_level3")?,
                        address_level2: row.get("l_address_level2")?,
                        address_level1: row.get("l_address_level1")?,
                        postal_code: row.get("l_postal_code")?,
                        country: row.get("l_country")?,
                        tel: row.get("l_tel")?,
                        email: row.get("l_email")?,
                        time_created: None,
                        time_last_used: None,
                        time_last_modified: None,
                        times_used: None,
                        sync_change_counter: None,
                    },
                    mirror: RecordData {
                        given_name: row.get("m_given_name")?,
                        additional_name: row.get("m_additional_name")?,
                        family_name: row.get("m_family_name")?,
                        organization: row.get("m_organization")?,
                        street_address: row.get("m_street_address")?,
                        address_level3: row.get("m_address_level3")?,
                        address_level2: row.get("m_address_level2")?,
                        address_level1: row.get("m_address_level1")?,
                        postal_code: row.get("m_postal_code")?,
                        country: row.get("m_country")?,
                        tel: row.get("m_tel")?,
                        email: row.get("m_email")?,
                        time_created: None,
                        time_last_used: None,
                        time_last_modified: None,
                        times_used: None,
                        sync_change_counter: None,
                    },
                },
            ))
        },
    )?)
}

#[allow(dead_code)]
fn get_incoming_record_states(conn: &Connection) -> Result<Vec<(SyncGuid, IncomingState)>> {
    let sql_query = "
        SELECT
            s.guid as s_guid,
            m.guid as m_guid,
            l.guid as l_guid,
            s.given_name as s_given_name,
            m.given_name as m_given_name,
			l.given_name as l_given_name,
            s.additional_name as s_additional_name,
			m.additional_name as m_additional_name,
			l.additional_name as l_additional_name,
            s.family_name as s_family_name,
			m.family_name as m_family_name,
			l.family_name as l_family_name,
            s.organization as s_organization,
			m.organization as m_organization,
			l.organization as l_organization,
            s.street_address as s_street_address,
			m.street_address as m_street_address,
			l.street_address as l_street_address,
            s.address_level3 as s_address_level3,
			m.address_level3 as m_address_level3,
			l.address_level3 as l_address_level3,
            s.address_level2 as s_address_level2,
			m.address_level2 as m_address_level2,
			l.address_level2 as l_address_level2,
            s.address_level1 as s_address_level1,
			m.address_level1 as m_address_level1,
			l.address_level1 as l_address_level1,
            s.postal_code as s_postal_code,
			m.postal_code as m_postal_code,
			l.postal_code as l_postal_code,
            s.country as s_country,
			m.country as m_country,
			l.country as l_country,
            s.tel as s_tel,
			m.tel as m_tel,
			l.tel as l_tel,
            s.email as s_email,
			m.email as m_email,
            l.email as l_email,
            l.time_created,
            l.time_last_used,
            l.time_last_modified,
            l.times_used,
            l.sync_change_counter
        FROM temp.addresses_sync_staging s
        LEFT JOIN addresses_mirror m ON s.guid = m.guid
        LEFT JOIN addresses_data l ON s.guid = l.guid";

    Ok(conn.conn().query_rows_and_then_named(
        sql_query,
        &[],
        |row| -> Result<(SyncGuid, IncomingState)> {
            let guid: String = row.get("s_guid")?;
            let mirror_guid: Option<String> = row.get("m_guid")?;
            let local_guid: Option<String> = row.get("l_guid")?;

            let incoming = RecordData {
                given_name: row.get("s_given_name")?,
                additional_name: row.get("s_additional_name")?,
                family_name: row.get("s_family_name")?,
                organization: row.get("s_organization")?,
                street_address: row.get("s_street_address")?,
                address_level3: row.get("s_address_level3")?,
                address_level2: row.get("s_address_level2")?,
                address_level1: row.get("s_address_level1")?,
                postal_code: row.get("s_postal_code")?,
                country: row.get("s_country")?,
                tel: row.get("s_tel")?,
                email: row.get("s_email")?,
                time_created: None,
                time_last_used: None,
                time_last_modified: None,
                times_used: None,
                sync_change_counter: None,
            };

            let mirror = match mirror_guid {
                Some(_) => Some(RecordData {
                    given_name: row.get("m_given_name")?,
                    additional_name: row.get("m_additional_name")?,
                    family_name: row.get("m_family_name")?,
                    organization: row.get("m_organization")?,
                    street_address: row.get("m_street_address")?,
                    address_level3: row.get("m_address_level3")?,
                    address_level2: row.get("m_address_level2")?,
                    address_level1: row.get("m_address_level1")?,
                    postal_code: row.get("m_postal_code")?,
                    country: row.get("m_country")?,
                    tel: row.get("m_tel")?,
                    email: row.get("m_email")?,
                    time_created: None,
                    time_last_used: None,
                    time_last_modified: None,
                    times_used: None,
                    sync_change_counter: None,
                }),
                None => None,
            };

            let incoming_state = match local_guid {
                Some(_) => IncomingState::HasLocal {
                    guid: guid.clone(),
                    incoming,
                    local: RecordData {
                        given_name: row.get("l_given_name")?,
                        additional_name: row.get("l_additional_name")?,
                        family_name: row.get("l_family_name")?,
                        organization: row.get("l_organization")?,
                        street_address: row.get("l_street_address")?,
                        address_level3: row.get("l_address_level3")?,
                        address_level2: row.get("l_address_level2")?,
                        address_level1: row.get("l_address_level1")?,
                        postal_code: row.get("l_postal_code")?,
                        country: row.get("l_country")?,
                        tel: row.get("l_tel")?,
                        email: row.get("l_email")?,
                        time_created: Some(row.get("time_created")?),
                        time_last_used: Some(row.get("time_last_used")?),
                        time_last_modified: Some(row.get("time_last_modified")?),
                        times_used: Some(row.get("times_used")?),
                        sync_change_counter: Some(row.get("sync_change_counter")?),
                    },
                    mirror,
                },
                None => {
                    let local_dupe = get_local_dupe(
                        conn,
                        Record {
                            guid: SyncGuid::from_string(guid.clone()),
                            data: incoming.clone(),
                        },
                    )?;

                    match local_dupe {
                        Some(d) => IncomingState::HasLocalDupe {
                            guid: guid.clone(),
                            incoming,
                            dupe_guid: d.guid.to_string(),
                            dupe: d.data,
                            mirror,
                        },
                        None => match has_local_tombstone(conn, &guid)? {
                            true => IncomingState::LocalTombstone {
                                guid: guid.clone(),
                                incoming,
                            },
                            false => IncomingState::IncomingOnly {
                                guid: guid.clone(),
                                incoming,
                            },
                        },
                    }
                }
            };

            Ok((SyncGuid::from_string(guid), incoming_state))
        },
    )?)
}

#[allow(dead_code)]
fn get_local_dupe(conn: &Connection, incoming: Record) -> Result<Option<Record>> {
    // find dupe matches desktop logic
    // https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillStorage.jsm#1240
    // which iterates over the record's fields, excluding the guid and sync metadata
    // fields, and returns the guid of any local record that has the same values for
    // all fields.

    let sql = "
        SELECT
            guid,
            given_name,
            additional_name,
            family_name,
            organization,
            street_address,
            address_level3,
            address_level2,
            address_level1,
            postal_code,
            country,
            tel,
            email,
            time_created,
            time_last_used,
            time_last_modified,
            times_used,
            sync_change_counter
        FROM addresses_data
        WHERE guid <> :guid
            AND guid NOT IN (
                SELECT guid
                FROM addresses_mirror
            )
            AND given_name == :given_name
            AND additional_name == :additional_name
            AND family_name == :family_name
            AND organization == :organization
            AND street_address == :street_address
            AND address_level3 == :address_level3
            AND address_level2 == :address_level2
            AND address_level1 == :address_level1
            AND postal_code == :postal_code
            AND country == :country
            AND tel == :tel
            AND email == :email";

    let params = named_params! {
        ":guid": incoming.guid.as_str(),
        ":given_name": incoming.data.given_name,
        ":additional_name": incoming.data.additional_name,
        ":family_name": incoming.data.family_name,
        ":organization": incoming.data.organization,
        ":street_address": incoming.data.street_address,
        ":address_level3": incoming.data.address_level3,
        ":address_level2": incoming.data.address_level2,
        ":address_level1": incoming.data.address_level1,
        ":postal_code": incoming.data.postal_code,
        ":country": incoming.data.country,
        ":tel": incoming.data.tel,
        ":email": incoming.data.email,
    };

    let result = conn.conn().query_row_named(&sql, params, |row| {
        Ok(Record {
            guid: row.get("guid")?,
            data: RecordData {
                given_name: row.get("given_name")?,
                additional_name: row.get("additional_name")?,
                family_name: row.get("family_name")?,
                organization: row.get("organization")?,
                street_address: row.get("street_address")?,
                address_level3: row.get("address_level3")?,
                address_level2: row.get("address_level2")?,
                address_level1: row.get("address_level1")?,
                postal_code: row.get("postal_code")?,
                country: row.get("country")?,
                tel: row.get("tel")?,
                email: row.get("email")?,
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    });

    match result {
        Ok(r) => Ok(Some(r)),
        Err(e) => match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            _ => Err(Error::SqlError(e)),
        },
    }
}

#[allow(dead_code)]
fn has_local_tombstone(conn: &Connection, guid: &str) -> Result<bool> {
    Ok(conn.conn().query_row(
        "SELECT EXISTS (
                SELECT 1
                FROM addresses_tombstones
                WHERE guid = :guid
            )",
        &[guid],
        |row| row.get(0),
    )?)
}

#[derive(Debug, PartialEq)]
pub enum IncomingAction {
    DeleteLocally {
        guid: SyncGuid,
        mirror: RecordData,
        changes: AddressChanges,
    },
    ReplaceLocal {
        old_guid: SyncGuid,
        new_guid: SyncGuid,
        changes: AddressChanges,
        data: RecordData,
    },
    TakeRemote {
        guid: SyncGuid,
        changes: AddressChanges,
        data: RecordData,
    },
    DeleteLocalTombstone {
        guid: SyncGuid,
        changes: AddressChanges,
        data: RecordData,
    },
    // Same { guid }, ?
    Nothing,
}

pub fn plan_incoming(s: IncomingState) -> IncomingAction {
    match s {
        IncomingState::IncomingOnly { guid, incoming } => IncomingAction::TakeRemote {
            guid: SyncGuid::new(&guid),
            changes: AddressChanges {
                guid: SyncGuid::new(&guid),
                old_value: RecordData {
                    ..Default::default()
                },
                new_value: Record {
                    guid: SyncGuid::new(&guid),
                    data: incoming.clone(),
                },
            },
            data: incoming,
        },
        IncomingState::IncomingTombstone {
            guid,
            local,
            mirror,
        } => IncomingAction::DeleteLocally {
            guid: SyncGuid::new(&guid),
            mirror,
            changes: AddressChanges {
                guid: SyncGuid::new(&guid),
                old_value: local,
                new_value: Record {
                    ..Default::default()
                },
            },
        },
        IncomingState::HasLocal {
            guid,
            incoming,
            local,
            mirror,
        } => {
            match local.sync_change_counter {
                Some(s) => match s == 0 {
                    true => IncomingAction::TakeRemote {
                        guid: SyncGuid::new(&guid),
                        changes: AddressChanges {
                            guid: SyncGuid::new(&guid),
                            old_value: local,
                            new_value: Record {
                                guid: SyncGuid::new(&guid),
                                data: incoming.clone(),
                            },
                        },
                        data: incoming,
                    },
                    false => merge(guid, incoming, local, mirror),
                },
                // the local record should always have a populated sync_change_counter property
                // but adding this to complete the match statement
                // TODO: what should happen here?
                None => IncomingAction::Nothing, //IncomingAction::Merge,
            }
        }

        // assign https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillStorage.jsm#1141
        IncomingState::HasLocalDupe {
            guid,
            incoming,
            dupe_guid: _,
            dupe,
            mirror,
        } => merge(guid, incoming, dupe, mirror),

        IncomingState::LocalTombstone { guid, incoming } => IncomingAction::DeleteLocalTombstone {
            guid: SyncGuid::new(&guid),
            changes: AddressChanges {
                guid: SyncGuid::new(&guid),
                new_value: Record {
                    guid: SyncGuid::new(&guid),
                    data: incoming.clone(),
                },
                old_value: RecordData {
                    ..Default::default()
                },
            },
            data: incoming,
        },
    }
}

fn merge(
    guid: String,
    incoming: RecordData,
    local: RecordData,
    mirror: Option<RecordData>,
) -> IncomingAction {
    let incoming_record = serde_json::to_value(&incoming).unwrap();
    let local_record = serde_json::to_value(&local).unwrap();
    let mut merged_value: RecordData;
    let mut merged_record = Map::new();

    let field_names = vec![
        "given-name",
        "additional-name",
        "family-name",
        "organization",
        "street-address",
        "address-level3",
        "address-level2",
        "address-level1",
        "postal-code",
        "country",
        "tel",
        "email",
    ];

    for field_name in field_names {
        let incoming_field = incoming_record.get(field_name).unwrap().to_string();
        let local_field = local_record.get(field_name).unwrap().to_string();
        let is_local_same;
        let is_incoming_same;

        match &mirror {
            Some(m) => {
                let mirror_field = serde_json::to_value(&m)
                    .unwrap()
                    .get(field_name)
                    .unwrap()
                    .to_string();
                is_local_same = mirror_field == local_field;
                is_incoming_same = mirror_field == incoming_field;
            }
            None => {
                is_local_same = true;
                is_incoming_same = local_field == incoming_field;
            }
        };

        let should_use_local = is_incoming_same || local == incoming.clone();

        if is_local_same && !is_incoming_same {
            merged_record.insert(String::from(field_name), Value::String(incoming_field));
        } else if should_use_local {
            merged_record.insert(String::from(field_name), Value::String(local_field));
        } else {
            //local and remote are different
            return get_forked_action(Record {
                guid: SyncGuid::new(&guid),
                data: local,
            });
        }
    }

    merged_value = serde_json::from_str(Value::Object(merged_record).as_str().unwrap()).unwrap();
    merged_value.time_created = incoming.time_created;
    merged_value.time_last_used = incoming.time_last_used;
    merged_value.time_last_modified = incoming.time_last_modified;
    merged_value.times_used = incoming.times_used;

    IncomingAction::ReplaceLocal {
        old_guid: SyncGuid::new(&guid),
        new_guid: SyncGuid::new(&guid),
        changes: AddressChanges {
            guid: SyncGuid::new(&guid),
            new_value: Record {
                guid: SyncGuid::new(&guid),
                data: merged_value.clone(),
            },
            old_value: local,
        },
        data: merged_value,
    }
}

fn get_forked_action(local_record: Record) -> IncomingAction {
    let local_record_data = local_record.clone().data;
    let forked_record = Record {
        guid: SyncGuid::random(),
        data: RecordData {
            given_name: local_record_data.given_name,
            additional_name: local_record_data.additional_name,
            family_name: local_record_data.family_name,
            organization: local_record_data.organization,
            street_address: local_record_data.street_address,
            address_level3: local_record_data.address_level3,
            address_level2: local_record_data.address_level2,
            address_level1: local_record_data.address_level1,
            postal_code: local_record_data.postal_code,
            country: local_record_data.country,
            tel: local_record_data.tel,
            email: local_record_data.email,
            time_created: Some(Timestamp::now()),
            time_last_used: Some(Timestamp::now()),
            time_last_modified: Some(Timestamp::now()),
            times_used: Some(0),
            sync_change_counter: Some(1),
        },
    };

    IncomingAction::ReplaceLocal {
        old_guid: local_record.guid.clone(),
        new_guid: forked_record.guid.clone(),
        changes: AddressChanges {
            guid: local_record.guid,
            new_value: Record {
                guid: forked_record.guid,
                data: forked_record.data.clone(),
            },
            old_value: local_record.data,
        },
        data: forked_record.data,
    }
}

fn insert_changes(conn: &Connection, changes: &AddressChanges) -> Result<()> {
    conn.execute_named(
        "INSERT OR IGNORE INTO addresses_data (
            guid,
            old_given_name,
            old_additional_name,
            old_family_name,
            old_organization,
            old_street_address,
            old_address_level3,
            old_address_level2,
            old_address_level1,
            old_postal_code,
            old_country,
            old_tel,
            old_email,
            new_guid,
            new_given_name,
            new_additional_name,
            new_family_name,
            new_organization,
            new_street_address,
            new_address_level3,
            new_address_level2,
            new_address_level1,
            new_postal_code,
            new_country,
            new_tel,
            new_email
        ) VALUES (
            :guid,
            :old_given_name,
            :old_additional_name,
            :old_family_name,
            :old_organization,
            :old_street_address,
            :old_address_level3,
            :old_address_level2,
            :old_address_level1,
            :old_postal_code,
            :old_country,
            :old_tel,
            :old_email,
            :new_guid,
            :new_given_name,
            :new_additional_name,
            :new_family_name,
            :new_organization,
            :new_street_address,
            :new_address_level3,
            :new_address_level2,
            :new_address_level1,
            :new_postal_code,
            :new_country,
            :new_tel,
            :new_email
        )",
        rusqlite::named_params! {
            ":guid": changes.guid.to_string(),
            ":old_given_name": changes.old_value.given_name,
            ":old_additional_name": changes.old_value.additional_name,
            ":old_family_name": changes.old_value.family_name,
            ":old_organization": changes.old_value.organization,
            ":old_street_address": changes.old_value.street_address,
            ":old_address_level3": changes.old_value.address_level3,
            ":old_address_level2": changes.old_value.address_level2,
            ":old_address_level1": changes.old_value.address_level1,
            ":old_postal_code": changes.old_value.postal_code,
            ":old_country": changes.old_value.country,
            ":old_tel": changes.old_value.tel,
            ":old_email": changes.old_value.email,
            ":new_guid": changes.new_value.guid,
            ":new_given_name": changes.new_value.data.given_name,
            ":new_additional_name": changes.new_value.data.additional_name,
            ":new_family_name": changes.new_value.data.family_name,
            ":new_organization": changes.new_value.data.organization,
            ":new_street_address": changes.new_value.data.street_address,
            ":new_address_level3": changes.new_value.data.address_level3,
            ":new_address_level2": changes.new_value.data.address_level2,
            ":new_address_level1": changes.new_value.data.address_level1,
            ":new_postal_code": changes.new_value.data.postal_code,
            ":new_country": changes.new_value.data.country,
            ":new_tel": changes.new_value.data.tel,
            ":new_email": changes.new_value.data.email,
        },
    )?;

    Ok(())
}

pub fn apply_actions(
    conn: &Connection,
    actions: Vec<(SyncGuid, IncomingAction)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    for (item, action) in actions {
        signal.err_if_interrupted()?;

        log::trace!("action for '{:?}': {:?}", item, action);
        match action {
            IncomingAction::DeleteLocally {
                guid: _,
                mirror: _,
                changes,
            } => {
                insert_changes(conn, &changes)?;
            }
            IncomingAction::ReplaceLocal {
                old_guid: _,
                new_guid: _,
                changes,
                data: _,
            } => {
                insert_changes(conn, &changes)?;
            }
            IncomingAction::TakeRemote {
                guid: _,
                changes,
                data: _,
            } => {
                insert_changes(conn, &changes)?;
            }
            IncomingAction::DeleteLocalTombstone {
                guid: _,
                changes,
                data: _,
            } => {
                insert_changes(conn, &changes)?;
            }
            IncomingAction::Nothing => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;

    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Value};

    fn array_to_incoming(mut array: Value) -> Vec<Payload> {
        let jv = array.as_array_mut().expect("you must pass a json array");
        let mut result = Vec::with_capacity(jv.len());
        for elt in jv {
            result.push(Payload::from_json(elt.take()).expect("must be valid"));
        }
        result
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        struct TestCase {
            incoming_records: Value,
            expected_record_count: u32,
            expected_tombstone_count: u32,
        }

        let test_cases = vec![
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAAAAA",
                        "deleted": false,
                        "givenName": "john",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "1300 Broadway",
                        "addressLevel3": "",
                        "addressLevel2": "New York, NY",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 1,
                expected_tombstone_count: 0,
            },
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAA",
                        "deleted": true,
                        "givenName": "",
                        "additionalName": "",
                        "familyName": "",
                        "organization": "",
                        "streetAddress": "",
                        "addressLevel3": "",
                        "addressLevel2": "",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAAAAA",
                        "deleted": false,
                        "givenName": "john",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "1300 Broadway",
                        "addressLevel3": "",
                        "addressLevel2": "New York, NY",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    },
                    {
                        "id": "CCCCCCCCCCCCCCCCCC",
                        "deleted": false,
                        "givenName": "jane",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "3050 South La Brea Ave",
                        "addressLevel3": "",
                        "addressLevel2": "Los Angeles, CA",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    },
                    {
                        "id": "BBBBBBBBBBBBBBBBB",
                        "deleted": true,
                        "givenName": "",
                        "additionalName": "",
                        "familyName": "",
                        "organization": "",
                        "streetAddress": "",
                        "addressLevel3": "",
                        "addressLevel2": "",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 2,
                expected_tombstone_count: 1,
            },
        ];

        for tc in test_cases {
            stage_incoming(
                &tx,
                array_to_incoming(tc.incoming_records),
                &NeverInterrupts,
            )?;

            let record_count: u32 = tx
                .try_query_one(
                    "SELECT COUNT(*) FROM temp.addresses_sync_staging",
                    &[],
                    false,
                )
                .expect("get incoming record count")
                .unwrap_or_default();

            let tombstone_count: u32 = tx
                .try_query_one(
                    "SELECT COUNT(*) FROM temp.addresses_tombstone_sync_staging",
                    &[],
                    false,
                )
                .expect("get incoming tombstone count")
                .unwrap_or_default();

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            tx.execute_all(&[
                "DELETE FROM temp.addresses_tombstone_sync_staging;",
                "DELETE FROM temp.addresses_sync_staging;",
            ])?;
        }
        Ok(())
    }

    #[test]
    fn test_get_incoming() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;

        tx.execute_named(
            "INSERT OR IGNORE INTO addresses_data (
                guid,
                given_name,
                additional_name,
                family_name,
                organization,
                street_address,
                address_level3,
                address_level2,
                address_level1,
                postal_code,
                country,
                tel,
                email,
                time_created,
                time_last_used,
                time_last_modified,
                times_used,
                sync_change_counter
            ) VALUES (
                :guid,
                :given_name,
                :additional_name,
                :family_name,
                :organization,
                :street_address,
                :address_level3,
                :address_level2,
                :address_level1,
                :postal_code,
                :country,
                :tel,
                :email,
                :time_created,
                :time_last_used,
                :time_last_modified,
                :times_used,
                :sync_change_counter
            )",
            rusqlite::named_params! {
                ":guid": "CCCCCCCCCCCCCCCCCC",
                ":given_name": "jane",
                ":additional_name": "",
                ":family_name": "doe",
                ":organization": "",
                ":street_address": "3050 South La Brea Ave",
                ":address_level3": "",
                ":address_level2": "Los Angeles, CA",
                ":address_level1": "",
                ":postal_code": "",
                ":country": "United States",
                ":tel": "",
                ":email": "",
                ":time_created": Timestamp::now(),
                ":time_last_used": Some(Timestamp::now()),
                ":time_last_modified": Timestamp::now(),
                ":times_used": 0,
                ":sync_change_counter": 1,
            },
        )?;

        tx.execute_named(
            "INSERT OR IGNORE INTO temp.addresses_sync_staging (
                guid,
                given_name,
                additional_name,
                family_name,
                organization,
                street_address,
                address_level3,
                address_level2,
                address_level1,
                postal_code,
                country,
                tel,
                email
            ) VALUES (
                :guid,
                :given_name,
                :additional_name,
                :family_name,
                :organization,
                :street_address,
                :address_level3,
                :address_level2,
                :address_level1,
                :postal_code,
                :country,
                :tel,
                :email
            )",
            rusqlite::named_params! {
                ":guid": "CCCCCCCCCCCCCCCCCC",
                ":given_name": "jane",
                ":additional_name": "",
                ":family_name": "doe",
                ":organization": "",
                ":street_address": "3050 South La Brea Ave",
                ":address_level3": "",
                ":address_level2": "Los Angeles, CA",
                ":address_level1": "",
                ":postal_code": "",
                ":country": "United States",
                ":tel": "",
                ":email": "",
            },
        )?;

        get_incoming(&tx)?;

        tx.execute_all(&[
            "DELETE FROM addresses_data;",
            "DELETE FROM temp.addresses_sync_staging;",
        ])?;
        // // assert!(false);

        Ok(())
    }
}
