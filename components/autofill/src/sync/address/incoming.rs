/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::{Record, RecordData};
use crate::error::*;
use interrupt_support::Interruptee;
use rusqlite::{named_params, types::ToSql, Connection};
use serde_json::{Map, Value};
use sql_support::ConnExt;
use sync15::Payload;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

/// The first step in the "apply incoming" process for syncing autofill address records.
/// Incoming tombstones will saved in the `temp.addresses_tombstone_sync_staging` table
/// and incoming records will be saved to the `temp.addresses_sync_staging` table.
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

/// Saves incoming records (excluding incoming tombstones) in preparation for applying
/// incoming changes for the syncing autofill address records.
fn save_incoming_records(
    conn: &Connection,
    incoming_records: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let chunk_size = 17;
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
                    email,
                    time_created,
                    time_last_used,
                    time_last_modified,
                    times_used
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
                params.push(&record.data.time_created);
                params.push(&record.data.time_last_used);
                params.push(&record.data.time_last_modified);
                params.push(&record.data.times_used);
            }
            conn.execute(&sql, &params)?;
            Ok(())
        },
    )
}

/// Saves incoming tombstones (excluding incoming records) in preparation for applying
/// incoming changes for the syncing autofill address records.
fn save_incoming_tombstones(
    conn: &Connection,
    incoming_tombstones: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
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

/// The distinct states of records to be synced which determine the `IncomingAction` to be taken.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum IncomingState {
    // Only the incoming record exists. An associated local or mirror record doesn't exist.
    IncomingOnly {
        guid: String,
        incoming: RecordData,
    },
    // The incoming record is a tombstone.
    IncomingTombstone {
        guid: String,
        local: Option<RecordData>,
        has_local_tombstone: bool,
    },
    // The incoming record has an associated local record.
    HasLocal {
        guid: String,
        incoming: RecordData,
        local: RecordData,
        mirror: Option<RecordData>,
    },
    // The incoming record doesn't have an associated local record with the same GUID.
    // A local record with the same data but a different GUID has been located.
    HasLocalDupe {
        guid: String,
        incoming: RecordData,
        dupe_guid: String,
        dupe: RecordData,
        mirror: Option<RecordData>,
    },
    // The incoming record doesn't have an associated local or local duplicate record but does
    // have a local tombstone.
    NonDeletedIncoming {
        guid: String,
        incoming: RecordData,
    },
}

/// The second step in the "apply incoming" process for syncing autofill address records.
/// Incoming tombstones and records are retrieved from the temp tables and assigned
/// `IncomingState` values.
pub fn get_incoming(conn: &Connection) -> Result<Vec<(SyncGuid, IncomingState)>> {
    let mut incoming_states = get_incoming_tombstone_states(conn)?;
    let mut incoming_record_states = get_incoming_record_states(conn)?;
    incoming_states.append(&mut incoming_record_states);

    Ok(incoming_states)
}

/// Incoming tombstones are retrieved from the `addresses_tombstone_sync_staging` table
/// and assigned `IncomingState` values.
#[allow(dead_code)]
fn get_incoming_tombstone_states(conn: &Connection) -> Result<Vec<(SyncGuid, IncomingState)>> {
    Ok(conn.conn().query_rows_and_then_named(
        "SELECT
            s.guid as s_guid,
            l.guid as l_guid,
            t.guid as t_guid,
            l.given_name,
            l.additional_name,
            l.family_name,
            l.organization,
            l.street_address,
            l.address_level3,
            l.address_level2,
            l.address_level1,
            l.postal_code,
            l.country,
            l.tel,
            l.email,
            l.time_created,
            l.time_last_used,
            l.time_last_modified,
            l.times_used,
            l.sync_change_counter
        FROM temp.addresses_tombstone_sync_staging s
        LEFT JOIN addresses_data l ON s.guid = l.guid
        LEFT JOIN addresses_tombstones t ON s.guid = t.guid",
        &[],
        |row| -> Result<(SyncGuid, IncomingState)> {
            let incoming_guid: String = row.get_unwrap("s_guid");
            let guid: SyncGuid = SyncGuid::from_string(incoming_guid.clone());
            let local_guid: Option<String> = row.get("l_guid")?;
            let tombstone_guid: Option<String> = row.get("t_guid")?;

            Ok((
                guid,
                IncomingState::IncomingTombstone {
                    guid: incoming_guid,
                    local: match local_guid {
                        Some(_) => Some(RecordData::from_row(row, "")?),
                        None => None,
                    },
                    has_local_tombstone: tombstone_guid.is_some(),
                },
            ))
        },
    )?)
}

/// Incoming records (excluding tombstones) are retrieved from the `addresses_sync_staging` table
/// and assigned `IncomingState` values.
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
            s.time_created as s_time_created,
            m.time_created as m_time_created,
            l.time_created as l_time_created,
            s.time_last_used as s_time_last_used,
            m.time_last_used as m_time_last_used,
            l.time_last_used as l_time_last_used,
            s.time_last_modified as s_time_last_modified,
            m.time_last_modified as m_time_last_modified,
            l.time_last_modified as l_time_last_modified,
            s.times_used as s_times_used,
            m.times_used as m_times_used,
            l.times_used as l_times_used,
            l.sync_change_counter as l_sync_change_counter
        FROM temp.addresses_sync_staging s
        LEFT JOIN addresses_mirror m ON s.guid = m.guid
        LEFT JOIN addresses_data l ON s.guid = l.guid";

    Ok(conn.conn().query_rows_and_then_named(
        sql_query,
        &[],
        |row| -> Result<(SyncGuid, IncomingState)> {
            let guid: String = row.get_unwrap("s_guid");
            let mirror_guid: Option<String> = row.get_unwrap("m_guid");
            let local_guid: Option<String> = row.get_unwrap("l_guid");

            let incoming = RecordData::from_row(row, "s_")?;

            let mirror = match mirror_guid {
                Some(_) => Some(RecordData::from_row(row, "m_")?),
                None => None,
            };

            let incoming_state = match local_guid {
                Some(_) => IncomingState::HasLocal {
                    guid: guid.clone(),
                    incoming,
                    local: RecordData::from_row(row, "l_")?,
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
                            true => IncomingState::NonDeletedIncoming {
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

/// Returns a local record that has the same values as the given incoming record (with the exception
/// of the `guid` values which should differ) that will be used as a local duplicate record for
/// syncing.
#[allow(dead_code)]
fn get_local_dupe(conn: &Connection, incoming: Record) -> Result<Option<Record>> {
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
            guid: row.get_unwrap("guid"),
            data: RecordData::from_row(&row, "")?,
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

/// Determines if a local tombstone exists for a given GUID.
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

/// The distinct incoming sync actions to be preformed for incoming records.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum IncomingAction {
    DeleteLocalRecord {
        guid: SyncGuid,
    },
    TakeMergedRecord {
        merged_record: Record,
    },
    UpdateLocalGuid {
        old_guid: String,
        dupe_guid: String,
        new_record: Record,
    },
    TakeRemote {
        new_record: Record,
    },
    DeleteLocalTombstone {
        remote_record: Record,
    },
    DoNothing,
}

/// Given an `IncomingState` returns the `IncomingAction` that should be performed.
pub fn plan_incoming(s: IncomingState) -> IncomingAction {
    match s {
        IncomingState::IncomingOnly { guid, incoming } => IncomingAction::TakeRemote {
            new_record: Record {
                guid: SyncGuid::new(&guid),
                data: incoming,
            },
        },
        IncomingState::IncomingTombstone {
            guid,
            local,
            has_local_tombstone,
        } => match local {
            Some(l) => {
                // Note: On desktop, when there's a local record for an incoming tombstone, a local tombstone
                // would created. But we don't actually need to create a local tombstone here. If we did it would
                // immediately be deleted after being uploaded to the server.
                let has_local_changes = l.sync_change_counter.unwrap_or(0) != 0;

                if has_local_changes || has_local_tombstone {
                    IncomingAction::DoNothing
                } else {
                    IncomingAction::DeleteLocalRecord {
                        guid: SyncGuid::new(&guid),
                    }
                }
            }
            None => IncomingAction::DoNothing,
        },
        IncomingState::HasLocal {
            guid,
            incoming,
            local,
            mirror,
        } => match local.sync_change_counter.unwrap_or(0) == 0 {
            true => IncomingAction::TakeRemote {
                new_record: Record {
                    guid: SyncGuid::new(&guid),
                    data: incoming,
                },
            },
            false => IncomingAction::TakeMergedRecord {
                merged_record: merge(guid, incoming, local, mirror),
            },
        },
        IncomingState::HasLocalDupe {
            guid,
            incoming,
            dupe_guid,
            dupe,
            mirror,
        } => {
            let new_record = merge(guid.clone(), incoming, dupe, mirror);
            IncomingAction::UpdateLocalGuid {
                old_guid: guid,
                dupe_guid,
                new_record,
            }
        }
        IncomingState::NonDeletedIncoming { guid, incoming } => {
            IncomingAction::DeleteLocalTombstone {
                remote_record: Record {
                    guid: SyncGuid::from_string(guid),
                    data: incoming,
                },
            }
        }
    }
}

/// Performs a three-way merge between an incoming, local, and mirror record. If a merge
/// cannot be successfully completed, the local record data is returned with a new guid
/// and sync metadata.
fn merge(
    guid: String,
    incoming: RecordData,
    local: RecordData,
    mirror: Option<RecordData>,
) -> Record {
    let i = serde_json::to_value(&incoming).unwrap();
    let l = serde_json::to_value(&local).unwrap();
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
        let incoming_field = i.get(field_name).unwrap().to_string();
        let local_field = l.get(field_name).unwrap().to_string();
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
            return get_forked_record(Record {
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

    Record {
        guid: SyncGuid::new(&guid),
        data: merged_value.clone(),
    }
}

/// Returns a with the given local record's data but with a new guid and
/// fresh sync metadata.
fn get_forked_record(local_record: Record) -> Record {
    let mut local_record_data = local_record.data;
    local_record_data.time_created = Timestamp::now();
    local_record_data.time_last_used = Timestamp::now();
    local_record_data.time_last_modified = Timestamp::now();
    local_record_data.times_used = 0;
    local_record_data.sync_change_counter = Some(1);

    Record {
        guid: SyncGuid::random(),
        data: local_record_data,
    }
}

/// Changes the guid of the local record for the given `old_guid` to the given `new_guid` used
/// for the `HasLocalDupe` incoming state.
fn change_local_guid(conn: &Connection, old_guid: String, new_guid: String) -> Result<()> {
    conn.conn().execute_named(
        "UPDATE addresses_data
        SET guid = :new_guid
        WHERE guid = :old_guid
        AND guid NOT IN (
            SELECT guid
            FROM addressess_mirror m
            WHERE m.guid = :old_guid
        )
        AND NOT EXISTS (
            SELECT 1
            FROM addresses_data d
            WHERE d.guid = :new_guid
        )",
        rusqlite::named_params! {
            ":old_guid": old_guid,
            ":new_guid": new_guid,
        },
    )?;

    Ok(())
}

fn update_local_record(conn: &Connection, new_record: Record) -> Result<()> {
    conn.execute_named(
        "UPDATE addresses_data
        SET given_name         = :given_name,
            additional_name     = :additional_name,
            family_name         = :family_name,
            organization        = :organization,
            street_address      = :street_address,
            address_level3      = :address_level3,
            address_level2      = :address_level2,
            address_level1      = :address_level1,
            postal_code         = :postal_code,
            country             = :country,
            tel                 = :tel,
            email               = :email,
            sync_change_counter = 0
        WHERE guid              = :guid",
        rusqlite::named_params! {
            ":given_name": new_record.data.given_name,
            ":additional_name": new_record.data.additional_name,
            ":family_name": new_record.data.family_name,
            ":organization": new_record.data.organization,
            ":street_address": new_record.data.street_address,
            ":address_level3": new_record.data.address_level3,
            ":address_level2": new_record.data.address_level2,
            ":address_level1": new_record.data.address_level1,
            ":postal_code": new_record.data.postal_code,
            ":country": new_record.data.country,
            ":tel": new_record.data.tel,
            ":email": new_record.data.email,
            ":guid": new_record.guid,
        },
    )?;

    Ok(())
}

fn insert_local_record(conn: &Connection, new_record: Record) -> Result<()> {
    conn.execute_named(
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
            ":guid": SyncGuid::random(),
            ":given_name": new_record.data.given_name,
            ":additional_name": new_record.data.additional_name,
            ":family_name": new_record.data.family_name,
            ":organization": new_record.data.organization,
            ":street_address": new_record.data.street_address,
            ":address_level3": new_record.data.address_level3,
            ":address_level2": new_record.data.address_level2,
            ":address_level1": new_record.data.address_level1,
            ":postal_code": new_record.data.postal_code,
            ":country": new_record.data.country,
            ":tel": new_record.data.tel,
            ":email": new_record.data.email,
            ":time_created": Timestamp::now(),
            ":time_last_used": Some(Timestamp::now()),
            ":time_last_modified": Timestamp::now(),
            ":times_used": 0,
            ":sync_change_counter": 0,
        },
    )?;

    Ok(())
}

fn upsert_local_record(conn: &Connection, new_record: Record) -> Result<()> {
    let exists = conn.query_row(
        "SELECT EXISTS (
            SELECT 1
            FROM addresses_data d
            WHERE guid = :guid
        )",
        &[new_record.clone().guid],
        |row| row.get(0),
    )?;

    if exists {
        update_local_record(conn, new_record)?;
    } else {
        insert_local_record(conn, new_record)?;
    }
    Ok(())
}

/// Apply the actions necessary to fully process the incoming items
pub fn apply_actions(
    conn: &Connection,
    actions: Vec<(SyncGuid, IncomingAction)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    for (item, action) in actions {
        signal.err_if_interrupted()?;

        log::trace!("action for '{:?}': {:?}", item, action);
        match action {
            IncomingAction::TakeMergedRecord { merged_record } => {
                update_local_record(conn, merged_record)?;
            }
            IncomingAction::UpdateLocalGuid {
                dupe_guid,
                old_guid,
                new_record,
            } => {
                change_local_guid(conn, old_guid, dupe_guid)?;
                update_local_record(conn, new_record)?;
            }
            IncomingAction::TakeRemote { new_record } => {
                upsert_local_record(conn, new_record)?;
            }
            IncomingAction::DeleteLocalTombstone { remote_record } => {
                conn.execute_named(
                    "DELETE FROM addresses_tombstones WHERE guid = :guid",
                    rusqlite::named_params! {
                        ":guid": remote_record.guid,
                    },
                )?;

                insert_local_record(conn, remote_record)?;
            }
            IncomingAction::DeleteLocalRecord { guid } => {
                conn.execute_named(
                    "DELETE FROM addresses_data
                    WHERE guid = :guid",
                    rusqlite::named_params! {
                        ":guid": guid,
                    },
                )?;
            }
            IncomingAction::DoNothing => {}
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
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
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
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
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
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
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
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
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
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
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
                email,
                time_created,
                time_last_used,
                time_last_modified,
                times_used
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
                :times_used
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
                ":time_created": 0,
                ":time_last_used": 0,
                ":time_last_modified": 0,
                ":times_used": 0,

            },
        )?;

        get_incoming(&tx)?;

        tx.execute_all(&[
            "DELETE FROM addresses_data;",
            "DELETE FROM temp.addresses_sync_staging;",
        ])?;

        Ok(())
    }
}
