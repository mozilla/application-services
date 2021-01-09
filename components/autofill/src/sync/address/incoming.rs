/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::{AddressChanges, Record, RecordData};
use crate::error::*;
use interrupt_support::Interruptee;
use rusqlite::{named_params, types::ToSql, Connection};
use serde_json::{json, Map, Value};
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
        incoming: Value,
    },
    IncomingTombstone {
        guid: String,
        local: Option<Value>,
        has_local_tombstone: bool,
    },
    HasLocal {
        guid: String,
        incoming: Value,
        local: Value,
        mirror: Option<Value>,
    },
    // In desktop, if a local record with the remote's guid doesn't exist, an attempt is made
    // to find a local dupe of the remote record
    // (https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillSync.jsm#132).
    // `HasLocalDupe` is the state which represents when said dupe is found. This is logic
    // that may need to be updated in the future, but currently exists solely for the purpose of reaching
    // parity with desktop.
    HasLocalDupe {
        guid: String,
        incoming: Value,
        dupe_guid: String,
        dupe: Value,
        mirror: Option<Value>,
    },
    LocalTombstone {
        guid: String,
        incoming: Value,
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
            s.guid as s_guid,
            l.guid as l_guid,
            t.guid as t_guid,
            l.given_name as l_given_name,
            l.additional_name as l_additional_name,
            l.family_name as l_family_name,
            l.organization as l_organization,
            l.street_address as l_street_address,
            l.address_level3 as l_address_level3,
            l.address_level2 as l_address_level2,
            l.address_level1 as l_address_level1,
            l.postal_code as l_postal_code,
            l.country as l_country,
            l.tel as l_tel,
            l.email as l_email,
            l.sync_change_counter as l_sync_change_counter
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
                        Some(_) => Some(json!({
                            "givenName": row.get_unwrap::<_, String>("l_given_name"),
                            "additionalName": row.get_unwrap::<_, String>("l_additional_name"),
                            "familyName": row.get_unwrap::<_, String>("l_family_name"),
                            "organization": row.get_unwrap::<_, String>("l_organization"),
                            "street_address": row.get_unwrap::<_, String>("l_street_address"),
                            "addressLevel3": row.get_unwrap::<_, String>("l_address_level3"),
                            "addressLevel2": row.get_unwrap::<_, String>("l_address_level2"),
                            "addressLevel1": row.get_unwrap::<_, String>("l_address_level1"),
                            "postalCode": row.get_unwrap::<_, String>("l_postal_code"),
                            "country": row.get_unwrap::<_, String>("l_country"),
                            "tel": row.get_unwrap::<_, String>("l_tel"),
                            "email": row.get_unwrap::<_, String>("l_email"),
                            "timeCreated": "",
                            "timeLastUsed": "",
                            "timeLastModified": "",
                            "timesUsed": "",
                            "syncChangeCounter": row.get_unwrap::<_, String>("l_sync_change_counter"),
                        })),
                        None => None,
                    },
                    has_local_tombstone: tombstone_guid.is_some(),
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
            let guid: String = row.get_unwrap("s_guid");
            let mirror_guid: Option<String> = row.get_unwrap("m_guid");
            let local_guid: Option<String> = row.get_unwrap("l_guid");

            let incoming = json!({
                "givenName": row.get_unwrap::<_, String>("s_given_name"),
                "additionalName": row.get_unwrap::<_, String>("s_additional_name"),
                "familyName": row.get_unwrap::<_, String>("s_family_name"),
                "organization": row.get_unwrap::<_, String>("s_organization"),
                "street_address": row.get_unwrap::<_, String>("s_street_address"),
                "addressLevel3": row.get_unwrap::<_, String>("s_address_level3"),
                "addressLevel2": row.get_unwrap::<_, String>("s_address_level2"),
                "addressLevel1": row.get_unwrap::<_, String>("s_address_level1"),
                "postalCode": row.get_unwrap::<_, String>("s_postal_code"),
                "country": row.get_unwrap::<_, String>("s_country"),
                "tel": row.get_unwrap::<_, String>("s_tel"),
                "email": row.get_unwrap::<_, String>("s_email"),
                "timeCreated": "",
                "timeLastUsed": "",
                "timeLastModified": "",
                "timesUsed": "",
                "syncChangeCounter": "",
            });

            let mirror = match mirror_guid {
                Some(_) => Some(json!({
                    "givenName": row.get_unwrap::<_, String>("m_given_name"),
                    "additionalName": row.get_unwrap::<_, String>("m_additional_name"),
                    "familyName": row.get_unwrap::<_, String>("m_family_name"),
                    "organization": row.get_unwrap::<_, String>("m_organization"),
                    "street_address": row.get_unwrap::<_, String>("m_street_address"),
                    "addressLevel3": row.get_unwrap::<_, String>("m_address_level3"),
                    "addressLevel2": row.get_unwrap::<_, String>("m_address_level2"),
                    "addressLevel1": row.get_unwrap::<_, String>("m_address_level1"),
                    "postalCode": row.get_unwrap::<_, String>("m_postal_code"),
                    "country": row.get_unwrap::<_, String>("m_country"),
                    "tel": row.get_unwrap::<_, String>("m_tel"),
                    "email": row.get_unwrap::<_, String>("m_email"),
                    "timeCreated": "",
                    "timeLastUsed": "",
                    "timeLastModified": "",
                    "timesUsed": "",
                    "syncChangeCounter": "",
                })),
                None => None,
            };

            let incoming_state = match local_guid {
                Some(_) => IncomingState::HasLocal {
                    guid: guid.clone(),
                    incoming,
                    local: json!({
                        "givenName": row.get_unwrap::<_, String>("l_given_name"),
                        "additionalName": row.get_unwrap::<_, String>("l_additional_name"),
                        "familyName": row.get_unwrap::<_, String>("l_family_name"),
                        "organization": row.get_unwrap::<_, String>("l_organization"),
                        "street_address": row.get_unwrap::<_, String>("l_street_address"),
                        "addressLevel3": row.get_unwrap::<_, String>("l_address_level3"),
                        "addressLevel2": row.get_unwrap::<_, String>("l_address_level2"),
                        "addressLevel1": row.get_unwrap::<_, String>("l_address_level1"),
                        "postalCode": row.get_unwrap::<_, String>("l_postal_code"),
                        "country": row.get_unwrap::<_, String>("l_country"),
                        "tel": row.get_unwrap::<_, String>("l_tel"),
                        "email": row.get_unwrap::<_, String>("l_email"),
                        "timeCreated": Some(row.get_unwrap::<_, i64>("time_created")),
                        "timeLastUsed": Some(row.get_unwrap::<_, i64>("time_last_used")),
                        "timeLastModified": Some(row.get_unwrap::<_, i64>("time_last_modified")),
                        "timesUsed": Some(row.get_unwrap::<_, i64>("times_used")),
                        "syncChangeCounter": Some(row.get_unwrap::<_, i64>("sync_change_counter")),
                    }),
                    mirror,
                },
                None => {
                    let local_dupe = get_local_dupe(
                        conn,
                        Record {
                            guid: SyncGuid::from_string(guid.clone()),
                            data: serde_json::from_value(incoming.clone()).unwrap(),
                        },
                    )?;

                    match local_dupe {
                        Some(d) => IncomingState::HasLocalDupe {
                            guid: guid.clone(),
                            incoming,
                            dupe_guid: d.guid.to_string(),
                            dupe: serde_json::to_value(&d.data).unwrap(),
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
            guid: row.get_unwrap("guid"),
            data: RecordData::from_row(&row)?,
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
        changes: Value,
    },
    CreateLocalTombstone {
        changes: Value,
        tombstone_is_synced: bool,
    },
    TakeMergedRecord {
        changes: Value,
    },
    UpdateLocalGuid {
        dupe_guid: String,
        changes: Value,
    },
    TakeRemote {
        changes: Value,
    },
    DeleteLocalTombstone {
        changes: Value,
    },
    DoNothing,
}

pub fn plan_incoming(s: IncomingState) -> IncomingAction {
    match s {
        IncomingState::IncomingOnly { guid, incoming } => IncomingAction::TakeRemote {
            changes: serde_json::to_value(&AddressChanges {
                old_value: None,
                new_value: Some(Record {
                    guid: SyncGuid::new(&guid),
                    data: RecordData {
                        given_name: incoming["givenName"].to_string(),
                        additional_name: incoming["additionalName"].to_string(),
                        family_name: incoming["familyName"].to_string(),
                        organization: incoming["organization"].to_string(),
                        street_address: incoming["streetAddress"].to_string(),
                        address_level3: incoming["addressLevel3"].to_string(),
                        address_level2: incoming["addressLevel2"].to_string(),
                        address_level1: incoming["addressLevel1"].to_string(),
                        postal_code: incoming["postalCode"].to_string(),
                        country: incoming["country"].to_string(),
                        tel: incoming["tel"].to_string(),
                        email: incoming["email"].to_string(),
                        time_created: Some(Timestamp(incoming["timeCreated"].as_u64().unwrap())),
                        time_last_used: Some(Timestamp(incoming["timeLastUsed"].as_u64().unwrap())),
                        time_last_modified: Some(Timestamp(
                            incoming["timeLastModified"].as_u64().unwrap(),
                        )),
                        times_used: Some(incoming["timesUsed"].as_i64().unwrap()),
                        sync_change_counter: Some(incoming["syncChangeCounter"].as_i64().unwrap()),
                    },
                }),
            })
            .unwrap(),
        },
        IncomingState::IncomingTombstone {
            guid,
            local,
            has_local_tombstone,
        } => match local {
            Some(l) => {
                let local_record: RecordData = serde_json::from_value(l).unwrap();
                let has_local_changes = local_record.sync_change_counter.is_some()
                    && local_record.sync_change_counter.unwrap() != 0;

                if has_local_changes || has_local_tombstone {
                    IncomingAction::DoNothing
                } else {
                    IncomingAction::CreateLocalTombstone {
                        changes: serde_json::to_value(&AddressChanges {
                            old_value: Some(Record {
                                guid: SyncGuid::new(&guid),
                                data: local_record,
                            }),
                            new_value: None,
                        })
                        .unwrap(),
                        tombstone_is_synced: true,
                    }
                }
            }
            None => IncomingAction::CreateLocalTombstone {
                changes: serde_json::to_value(&AddressChanges {
                    old_value: None,
                    new_value: None,
                })
                .unwrap(),
                tombstone_is_synced: false,
            },
        },
        IncomingState::HasLocal {
            guid,
            incoming,
            local,
            mirror,
        } => {
            let old_record_data: RecordData = serde_json::from_value(local.clone()).unwrap();
            let old_value: Option<Record> = Some(Record {
                guid: SyncGuid::from_string(guid.clone()),
                data: old_record_data.clone(),
            });

            let sync_change_counter = old_record_data.sync_change_counter.unwrap();
            match sync_change_counter == 0 {
                true => IncomingAction::TakeRemote {
                    changes: serde_json::to_value(&AddressChanges {
                        old_value,
                        new_value: Some(Record {
                            guid: SyncGuid::new(&guid),
                            data: serde_json::from_value(incoming).unwrap(),
                        }),
                    })
                    .unwrap(),
                },
                false => {
                    let new_value = Some(merge(guid, incoming, local, mirror));
                    IncomingAction::TakeMergedRecord {
                        changes: serde_json::to_value(&AddressChanges {
                            new_value,
                            old_value,
                        })
                        .unwrap(),
                    }
                }
            }
        }
        IncomingState::HasLocalDupe {
            guid,
            incoming,
            dupe_guid,
            dupe,
            mirror,
        } => {
            let new_value = Some(merge(guid.clone(), incoming, dupe.clone(), mirror));
            IncomingAction::UpdateLocalGuid {
                dupe_guid,
                changes: serde_json::to_value(&AddressChanges {
                    new_value,
                    old_value: Some(Record {
                        guid: SyncGuid::new(&guid),
                        data: serde_json::from_value(dupe).unwrap(),
                    }),
                })
                .unwrap(),
            }
        }
        IncomingState::LocalTombstone { guid, incoming } => IncomingAction::DeleteLocalTombstone {
            changes: serde_json::to_value(&AddressChanges {
                old_value: None,
                new_value: Some(Record {
                    guid: SyncGuid::new(&guid),
                    data: serde_json::from_value(incoming).unwrap(),
                }),
            })
            .unwrap(),
        },
    }
}

fn merge(guid: String, incoming: Value, local: Value, mirror: Option<Value>) -> Record {
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
        let incoming_field = incoming.get(field_name).unwrap().to_string();
        let local_field = local.get(field_name).unwrap().to_string();
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
            let data: RecordData = serde_json::from_value(local).unwrap();
            return get_forked_action(Record {
                guid: SyncGuid::new(&guid),
                data,
            });
        }
    }

    merged_value = serde_json::from_str(Value::Object(merged_record).as_str().unwrap()).unwrap();
    merged_value.time_created = Some(Timestamp(incoming["time_created"].as_u64().unwrap()));
    merged_value.time_last_used = Some(Timestamp(incoming["time_last_used"].as_u64().unwrap()));
    merged_value.time_last_modified =
        Some(Timestamp(incoming["time_last_modified"].as_u64().unwrap()));
    merged_value.times_used = Some(incoming["times_used"].as_i64().unwrap());

    Record {
        guid: SyncGuid::new(&guid),
        data: merged_value.clone(),
    }
}

fn get_forked_action(local_record: Record) -> Record {
    let mut local_record_data = local_record.data;
    local_record_data.time_created = Some(Timestamp::now());
    local_record_data.time_last_used = Some(Timestamp::now());
    local_record_data.time_last_modified = Some(Timestamp::now());
    local_record_data.times_used = Some(0);
    local_record_data.sync_change_counter = Some(1);

    Record {
        guid: SyncGuid::random(),
        data: local_record_data,
    }
}

fn insert_changes(conn: &Connection, changes: Value) -> Result<()> {
    let c: AddressChanges = serde_json::from_value(changes).unwrap();
    let sql = "INSERT OR IGNORE INTO addresses_data (
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
    )";

    let mut params: Vec<(&str, &dyn ToSql)> = Vec::new();

    if let Some(old_value) = c.old_value.as_ref() {
        let old_guid = &old_value.guid;
        params.push(("old_guid", old_guid));
        params.push((":old_given_name", &old_value.data.given_name));
        params.push((":old_additional_name", &old_value.data.additional_name));
        params.push((":old_family_name", &old_value.data.family_name));
        params.push((":old_organization", &old_value.data.organization));
        params.push((":old_street_address", &old_value.data.street_address));
        params.push((":old_address_level3", &old_value.data.address_level3));
        params.push((":old_address_level2", &old_value.data.address_level2));
        params.push((":old_address_level1", &old_value.data.address_level1));
        params.push((":old_postal_code", &old_value.data.postal_code));
        params.push((":old_country", &old_value.data.country));
        params.push((":old_tel", &old_value.data.tel));
        params.push((":old_email", &old_value.data.email));
    }

    if let Some(new_value) = c.new_value.as_ref() {
        let new_guid = &new_value.guid;
        params.push(("new_guid", new_guid));
        params.push(("new_given_name", &new_value.data.given_name));
        params.push((":new_additional_name", &new_value.data.additional_name));
        params.push((":new_family_name", &new_value.data.family_name));
        params.push((":new_organization", &new_value.data.organization));
        params.push((":new_street_address", &new_value.data.street_address));
        params.push((":new_address_level3", &new_value.data.address_level3));
        params.push((":new_address_level2", &new_value.data.address_level2));
        params.push((":new_address_level1", &new_value.data.address_level1));
        params.push((":new_postal_code", &new_value.data.postal_code));
        params.push((":new_country", &new_value.data.country));
        params.push((":new_tel", &new_value.data.tel));
        params.push((":new_email", &new_value.data.email));
    }

    conn.execute_named(sql, params.as_slice())?;
    Ok(())
}

fn change_local_guid(conn: &Connection, old_guid: String, new_guid: String) -> Result<()> {
    //TODO: https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillStorage.jsm#1141
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

pub fn apply_actions(
    conn: &Connection,
    actions: Vec<(SyncGuid, IncomingAction)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    for (item, action) in actions {
        signal.err_if_interrupted()?;

        log::trace!("action for '{:?}': {:?}", item, action);
        match action {
            IncomingAction::DeleteLocally { changes } => {
                insert_changes(conn, changes)?;
            }
            IncomingAction::TakeMergedRecord { changes } => {
                insert_changes(conn, changes)?;
            }
            IncomingAction::UpdateLocalGuid { dupe_guid, changes } => {
                let c: AddressChanges = serde_json::from_value(changes.clone()).unwrap();
                let old_guid = c.old_value.unwrap().guid.to_string();
                change_local_guid(conn, old_guid, dupe_guid)?;
                insert_changes(conn, changes)?;
            }
            IncomingAction::TakeRemote { changes } => {
                insert_changes(conn, changes)?;
            }
            IncomingAction::DeleteLocalTombstone { changes } => {
                insert_changes(conn, changes)?;
            }
            IncomingAction::CreateLocalTombstone {
                changes,
                tombstone_is_synced: _,
            } => {
                insert_changes(conn, changes)?;
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

        // assert!(false);

        Ok(())
    }
}
