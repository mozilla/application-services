/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::AddressRecord;
use crate::error::*;
use crate::sync::{MergeResult, RecordImpl};
use interrupt_support::Interruptee;
use rusqlite::{named_params, types::ToSql, Connection};
use sql_support::ConnExt;
use sync15::Payload;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

type IncomingState = crate::sync::IncomingState<AddressRecord>;
type IncomingRecordInfo = crate::sync::IncomingRecordInfo<AddressRecord>;
type LocalRecordInfo = crate::sync::LocalRecordInfo<AddressRecord>;
type IncomingAction = crate::sync::IncomingAction<AddressRecord>;

/// The first step in the "apply incoming" process for syncing autofill address records.
/// Incoming tombstones will saved in the `temp.addresses_tombstone_sync_staging` table
/// and incoming records will be saved to the `temp.addresses_sync_staging` table.
// XXX - will end up moving to the Impl trait??
pub fn stage_incoming(
    conn: &Connection,
    incoming_payloads: Vec<Payload>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let mut incoming_records = Vec::with_capacity(incoming_payloads.len());
    let mut incoming_tombstones = Vec::with_capacity(incoming_payloads.len());

    for payload in incoming_payloads {
        let payload_id = payload.id.clone();
        let payload_is_tombstone = payload.deleted;
        log::trace!(
            "incoming payload {} (deleted={})",
            payload_id,
            payload_is_tombstone,
        );

        match payload.into_record::<AddressRecord>() {
            Ok(address) => {
                if payload_is_tombstone {
                    incoming_tombstones.push(address);
                } else {
                    incoming_records.push(address);
                }
            }
            Err(e) => log::error!(
                "failed to deserialize incoming payload {} into `AddressRecord`, {}",
                payload_id,
                e,
            ),
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
    incoming_records: Vec<AddressRecord>,
    signal: &dyn Interruptee,
) -> Result<()> {
    log::info!("staging {} incoming records", incoming_records.len());
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
                params.push(&record.given_name);
                params.push(&record.additional_name);
                params.push(&record.family_name);
                params.push(&record.organization);
                params.push(&record.street_address);
                params.push(&record.address_level3);
                params.push(&record.address_level2);
                params.push(&record.address_level1);
                params.push(&record.postal_code);
                params.push(&record.country);
                params.push(&record.tel);
                params.push(&record.email);
                params.push(&record.time_created);
                params.push(&record.time_last_used);
                params.push(&record.time_last_modified);
                params.push(&record.times_used);
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
    incoming_tombstones: Vec<AddressRecord>,
    signal: &dyn Interruptee,
) -> Result<()> {
    log::info!("staging {} incoming tombstones", incoming_tombstones.len());
    sql_support::each_chunk(&incoming_tombstones, |chunk, _| -> Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO temp.addresses_tombstone_sync_staging (
                    guid
                ) VALUES {}",
            sql_support::repeat_sql_values(chunk.len())
        );
        signal.err_if_interrupted()?;

        let params: Vec<&dyn ToSql> = chunk.iter().map(|r| &r.guid as &dyn ToSql).collect();
        conn.execute(&sql, &params)?;
        Ok(())
    })
}

/// Incoming tombstones are retrieved from the `addresses_tombstone_sync_staging` table
/// and assigned `IncomingState` values.
fn get_incoming_tombstone_states(conn: &Connection) -> Result<Vec<IncomingState>> {
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
        |row| -> Result<IncomingState> {
            let incoming_guid: SyncGuid = row.get_unwrap("s_guid");
            let have_local_record = row.get::<_, Option<SyncGuid>>("l_guid")?.is_some();
            let have_local_tombstone = row.get::<_, Option<SyncGuid>>("t_guid")?.is_some();

            let local = if have_local_record {
                let record = AddressRecord::from_row(row, "")?;
                let has_local_changes = record.sync_change_counter.unwrap_or(0) != 0;
                if has_local_changes {
                    LocalRecordInfo::Modified { record }
                } else {
                    LocalRecordInfo::Unmodified { record }
                }
            } else if have_local_tombstone {
                LocalRecordInfo::Tombstone {
                    guid: incoming_guid.clone(),
                }
            } else {
                LocalRecordInfo::Missing
            };
            Ok(IncomingState {
                incoming: IncomingRecordInfo::Tombstone {
                    guid: incoming_guid,
                },
                local,
                mirror: None, // XXX - we *might* have a mirror record, but don't really care.
            })
        },
    )?)
}

/// Incoming records (excluding tombstones) are retrieved from the `addresses_sync_staging` table
/// and assigned `IncomingState` values.
fn get_incoming_record_states(conn: &Connection) -> Result<Vec<IncomingState>> {
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
            l.sync_change_counter as l_sync_change_counter,
            t.guid as t_guid
        FROM temp.addresses_sync_staging s
        LEFT JOIN addresses_mirror m ON s.guid = m.guid
        LEFT JOIN addresses_data l ON s.guid = l.guid
        LEFT JOIN addresses_tombstones t ON s.guid = t.guid";

    Ok(conn
        .conn()
        .query_rows_and_then_named(sql_query, &[], |row| -> Result<IncomingState> {
            let mirror_exists: bool = row.get::<_, SyncGuid>("m_guid").is_ok();
            let local_exists: bool = row.get::<_, SyncGuid>("l_guid").is_ok();
            let tombstone_guid: Option<SyncGuid> = row.get_unwrap("t_guid");

            let incoming = AddressRecord::from_row(row, "s_")?;
            let mirror = if mirror_exists {
                Some(AddressRecord::from_row(row, "m_")?)
            } else {
                None
            };
            let local = if local_exists {
                let record = AddressRecord::from_row(row, "l_")?;
                let has_changes = record.sync_change_counter.unwrap_or(0) != 0;
                if has_changes {
                    LocalRecordInfo::Modified { record }
                } else {
                    LocalRecordInfo::Unmodified { record }
                }
            } else {
                match tombstone_guid {
                    None => LocalRecordInfo::Missing,
                    Some(guid) => LocalRecordInfo::Tombstone { guid },
                }
            };

            Ok(IncomingState {
                incoming: IncomingRecordInfo::Record { record: incoming },
                local,
                mirror,
            })
        })?)
}

// A macro for our merge implementation.
// We allow all "common" fields from the sub-types to be getters on the
// InsertableItem type.
// This will probably move to the parent module?
macro_rules! field_check {
    ($field_name:ident,
    $incoming:ident,
    $local:ident,
    $mirror:ident,
    $merged_record:ident
    ) => {
        let incoming_field = &$incoming.$field_name;
        let local_field = &$local.$field_name;
        let is_local_same;
        let is_incoming_same;

        match &$mirror {
            Some(m) => {
                let mirror_field = &m.$field_name;
                is_local_same = mirror_field == local_field;
                is_incoming_same = mirror_field == incoming_field;
            }
            None => {
                is_local_same = true;
                is_incoming_same = local_field == incoming_field;
            }
        };

        let should_use_local = is_incoming_same || local_field == incoming_field;

        if is_local_same && !is_incoming_same {
            $merged_record.$field_name = incoming_field.clone();
        } else if should_use_local {
            $merged_record.$field_name = local_field.clone();
        } else {
            // There are conflicting differences, so we "fork" the record - we
            // will end up giving the local one a new guid and save the remote
            // one with its incoming ID.
            return MergeResult::Forked {
                forked: get_forked_record($local.clone()),
            };
        }
    };
}

pub struct AddressesImpl {}

impl RecordImpl for AddressesImpl {
    type Record = AddressRecord;

    /// The second step in the "apply incoming" process for syncing autofill address records.
    /// Incoming tombstones and records are retrieved from the temp tables and assigned
    /// `IncomingState` values.
    fn fetch_incoming_states(&self, conn: &Connection) -> Result<Vec<IncomingState>> {
        let mut incoming_infos = get_incoming_tombstone_states(conn)?;
        let mut incoming_record_infos = get_incoming_record_states(conn)?;
        incoming_infos.append(&mut incoming_record_infos);
        Ok(incoming_infos)
    }

    /// Performs a three-way merge between an incoming, local, and mirror record.
    /// If a merge cannot be successfully completed (ie, if we find the same
    /// field has changed both locally and remotely since the last sync), the
    /// local record data is returned with a new guid and updated sync metadata.
    /// Note that mirror being None is an edge-case and typically means first
    /// sync since a "reset" (eg, disconnecting and reconnecting.
    #[allow(clippy::cognitive_complexity)] // Looks like clippy considers this after macro-expansion...
    fn merge(
        &self,
        incoming: &Self::Record,
        local: &Self::Record,
        mirror: &Option<Self::Record>,
    ) -> MergeResult<Self::Record> {
        let mut merged_record: Self::Record = Default::default();
        // guids must be identical
        assert_eq!(incoming.guid, local.guid);

        match mirror {
            Some(m) => assert_eq!(incoming.guid, m.guid),
            None => {}
        };

        merged_record.guid = incoming.guid.clone();

        field_check!(given_name, incoming, local, mirror, merged_record);
        field_check!(additional_name, incoming, local, mirror, merged_record);
        field_check!(family_name, incoming, local, mirror, merged_record);
        field_check!(organization, incoming, local, mirror, merged_record);
        field_check!(street_address, incoming, local, mirror, merged_record);
        field_check!(address_level3, incoming, local, mirror, merged_record);
        field_check!(address_level2, incoming, local, mirror, merged_record);
        field_check!(address_level1, incoming, local, mirror, merged_record);
        field_check!(postal_code, incoming, local, mirror, merged_record);
        field_check!(country, incoming, local, mirror, merged_record);
        field_check!(tel, incoming, local, mirror, merged_record);
        field_check!(email, incoming, local, mirror, merged_record);

        merged_record.time_created = incoming.time_created;
        merged_record.time_last_used = incoming.time_last_used;
        merged_record.time_last_modified = incoming.time_last_modified;
        merged_record.times_used = incoming.times_used;

        self.merge_metadata(&mut merged_record, &local, &mirror);

        MergeResult::Merged {
            merged: merged_record,
        }
    }

    /// Merge the metadata from 2 or 3 records. `result` is one of the 2 or
    /// 3, so must already have valid metadata. It doesn't matter whether this
    /// is the local or incoming records, so long as it's different from `result`.
    /// Note that mirror being None is an edge-case and typically means first
    /// sync since a "reset" (eg, disconnecting and reconnecting.

    fn merge_metadata(
        &self,
        result: &mut AddressRecord,
        other: &AddressRecord,
        mirror: &Option<AddressRecord>,
    ) {
        match mirror {
            Some(m) => {
                fn get_latest_time(t1: Timestamp, t2: Timestamp, t3: Timestamp) -> Timestamp {
                    std::cmp::max(t1, std::cmp::max(t2, t3))
                }
                fn get_earliest_time(t1: Timestamp, t2: Timestamp, t3: Timestamp) -> Timestamp {
                    std::cmp::min(t1, std::cmp::min(t2, t3))
                }
                result.time_created =
                    get_earliest_time(result.time_created, other.time_created, m.time_created);
                result.time_last_used = get_latest_time(
                    result.time_last_used,
                    other.time_last_used,
                    m.time_last_used,
                );
                result.time_last_modified = get_latest_time(
                    result.time_last_modified,
                    other.time_last_modified,
                    m.time_last_modified,
                );

                result.times_used = m.times_used
                    + std::cmp::max(other.times_used - m.times_used, 0)
                    + std::cmp::max(result.times_used - m.times_used, 0);
            }
            None => {
                fn get_latest_time(t1: Timestamp, t2: Timestamp) -> Timestamp {
                    std::cmp::max(t1, t2)
                }
                fn get_earliest_time(t1: Timestamp, t2: Timestamp) -> Timestamp {
                    std::cmp::min(t1, t2)
                }
                result.time_created = get_earliest_time(result.time_created, other.time_created);
                result.time_last_used =
                    get_latest_time(result.time_last_used, other.time_last_used);
                result.time_last_modified =
                    get_latest_time(result.time_last_modified, other.time_last_modified);
                // No mirror is an edge-case that almost certainly means the
                // client was disconnected and this is the first sync after
                // reconnection. So we can't really do a simple sum() of the
                // times_used values as if the disconnection was recent, it will
                // be double the expected value.
                // So we just take the largest.
                result.times_used = std::cmp::max(other.times_used, result.times_used);
            }
        }
    }

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(
        &self,
        conn: &Connection,
        incoming: &Self::Record,
    ) -> Result<Option<(SyncGuid, Self::Record)>> {
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
            WHERE
                -- `guid <> :guid` is a pre-condition for this being called, but...
                guid <> :guid
                -- only non-synced records are candidates, which means can't already be in the mirror.
                AND guid NOT IN (
                    SELECT guid
                    FROM addresses_mirror
                )
                -- and sql can check the field values.
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
            ":guid": incoming.guid,
            ":given_name": incoming.given_name,
            ":additional_name": incoming.additional_name,
            ":family_name": incoming.family_name,
            ":organization": incoming.organization,
            ":street_address": incoming.street_address,
            ":address_level3": incoming.address_level3,
            ":address_level2": incoming.address_level2,
            ":address_level1": incoming.address_level1,
            ":postal_code": incoming.postal_code,
            ":country": incoming.country,
            ":tel": incoming.tel,
            ":email": incoming.email,
        };

        let result = conn.conn().query_row_named(&sql, params, |row| {
            Ok(AddressRecord::from_row(&row, "").expect("wtf? '?' doesn't work :("))
        });

        match result {
            Ok(r) => Ok(Some((incoming.guid.clone(), r))),
            Err(e) => match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(Error::SqlError(e)),
            },
        }
    }

    fn apply_action(&self, conn: &Connection, action: IncomingAction) -> Result<()> {
        log::trace!("applying action: {:?}", action);
        match action {
            IncomingAction::Update { record, was_merged } => {
                update_local_record(conn, record, was_merged)?;
            }
            IncomingAction::Fork { forked, incoming } => {
                // `forked` exists in the DB with the same guid as `incoming`, so fix that.
                change_local_guid(conn, &incoming.guid, &forked.guid)?;
                // `incoming` has the correct new guid.
                insert_local_record(conn, incoming)?;
            }
            IncomingAction::Insert { record } => {
                insert_local_record(conn, record)?;
            }
            IncomingAction::UpdateLocalGuid { old_guid, record } => {
                // expect record to have the new guid.
                assert_ne!(old_guid, record.guid);
                change_local_guid(conn, &old_guid, &record.guid)?;
                // the item is identical with the item with the new guid
                // *except* for the metadata - so we still need to update, but
                // don't need to treat the item as dirty.
                update_local_record(conn, record, false)?;
            }
            IncomingAction::ResurrectLocalTombstone { record } => {
                conn.execute_named(
                    "DELETE FROM addresses_tombstones WHERE guid = :guid",
                    rusqlite::named_params! {
                        ":guid": record.guid,
                    },
                )?;
                insert_local_record(conn, record)?;
            }
            IncomingAction::ResurrectRemoteTombstone { record } => {
                // This is just "ensure local record dirty", which
                // update_local_record conveniently does.
                update_local_record(conn, record, true)?;
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
        Ok(())
    }
}

/// Returns a with the given local record's data but with a new guid and
/// fresh sync metadata.
fn get_forked_record(local_record: AddressRecord) -> AddressRecord {
    let mut local_record_data = local_record;
    local_record_data.guid = SyncGuid::random();
    local_record_data.time_created = Timestamp::now();
    local_record_data.time_last_used = Timestamp::now();
    local_record_data.time_last_modified = Timestamp::now();
    local_record_data.times_used = 0;
    local_record_data.sync_change_counter = Some(1);

    local_record_data
}

/// Changes the guid of the local record for the given `old_guid` to the given `new_guid` used
/// for the `HasLocalDupe` incoming state, and mark the item as dirty.
fn change_local_guid(conn: &Connection, old_guid: &SyncGuid, new_guid: &SyncGuid) -> Result<()> {
    assert_ne!(old_guid, new_guid);
    let nrows = conn.conn().execute_named(
        "UPDATE addresses_data
        SET guid = :new_guid,
            sync_change_counter = sync_change_counter + 1
        WHERE guid = :old_guid
        ",
        rusqlite::named_params! {
            ":old_guid": old_guid,
            ":new_guid": new_guid,
        },
    )?;
    // something's gone badly wrong if this didn't affect exactly 1 row.
    assert_eq!(nrows, 1);
    Ok(())
}

fn update_local_record(
    conn: &Connection,
    new_record: AddressRecord,
    flag_as_changed: bool,
) -> Result<()> {
    let rows_changed = conn.execute_named(
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
            time_created        = :time_created,
            time_last_used      = :time_last_used,
            time_last_modified  = :time_last_modified,
            times_used          = :times_used,
            sync_change_counter = sync_change_counter + :change_counter_incr
        WHERE guid              = :guid",
        rusqlite::named_params! {
            ":given_name": new_record.given_name,
            ":additional_name": new_record.additional_name,
            ":family_name": new_record.family_name,
            ":organization": new_record.organization,
            ":street_address": new_record.street_address,
            ":address_level3": new_record.address_level3,
            ":address_level2": new_record.address_level2,
            ":address_level1": new_record.address_level1,
            ":postal_code": new_record.postal_code,
            ":country": new_record.country,
            ":tel": new_record.tel,
            ":email": new_record.email,
            ":time_created": new_record.time_created,
            ":time_last_used": new_record.time_last_used,
            ":time_last_modified": new_record.time_last_modified,
            ":times_used": new_record.times_used,
            ":guid": new_record.guid,
            ":change_counter_incr": flag_as_changed as u32,
        },
    )?;
    // if we didn't actually update a row them something has gone very wrong...
    assert_eq!(rows_changed, 1);
    Ok(())
}

fn insert_local_record(conn: &Connection, new_record: AddressRecord) -> Result<()> {
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
            ":guid": new_record.guid,
            ":given_name": new_record.given_name,
            ":additional_name": new_record.additional_name,
            ":family_name": new_record.family_name,
            ":organization": new_record.organization,
            ":street_address": new_record.street_address,
            ":address_level3": new_record.address_level3,
            ":address_level2": new_record.address_level2,
            ":address_level1": new_record.address_level1,
            ":postal_code": new_record.postal_code,
            ":country": new_record.country,
            ":tel": new_record.tel,
            ":email": new_record.email,
            ":time_created": new_record.time_created,
            ":time_last_used": new_record.time_last_used,
            ":time_last_modified": new_record.time_last_modified,
            ":times_used": new_record.times_used,
            ":sync_change_counter": 0,
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;

    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Map, Value};

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            let val = json! {{
                "A" : {
                    "id": expand_test_guid('A'),
                    "givenName": "john",
                    "familyName": "doe",
                    "streetAddress": "1300 Broadway",
                    "addressLevel2": "New York, NY",
                    "country": "United States",
                },
                "C" : {
                    "id": expand_test_guid('C'),
                    "givenName": "jane",
                    "familyName": "doe",
                    "streetAddress": "3050 South La Brea Ave",
                    "addressLevel2": "Los Angeles, CA",
                    "country": "United States",
                    "timeCreated": 0,
                    "timeLastUsed": 0,
                    "timeLastModified": 0,
                    "timesUsed": 0,
                }
            }};
            val.as_object().expect("literal is an object").clone()
        };
    }

    fn array_to_incoming(vals: Vec<Value>) -> Vec<Payload> {
        let mut result = Vec::with_capacity(vals.len());
        for elt in vals {
            result.push(Payload::from_json(elt.clone()).expect("must be valid"));
        }
        result
    }

    fn expand_test_guid(c: char) -> String {
        c.to_string().repeat(12)
    }

    fn test_json_record(guid_prefix: char) -> Value {
        TEST_JSON_RECORDS
            .get(&guid_prefix.to_string())
            .expect("should exist")
            .clone()
    }

    fn test_record(guid_prefix: char) -> AddressRecord {
        let json = test_json_record(guid_prefix);
        serde_json::from_value(json).expect("should be a valid record")
    }

    fn test_json_tombstone(guid_prefix: char) -> Value {
        let t = json! {
            {
                "id": expand_test_guid(guid_prefix),
                "deleted": true,
            }
        };
        t
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        struct TestCase {
            incoming_records: Vec<Value>,
            expected_record_count: u32,
            expected_tombstone_count: u32,
        }

        let test_cases = vec![
            TestCase {
                incoming_records: vec![test_json_record('A')],
                expected_record_count: 1,
                expected_tombstone_count: 0,
            },
            TestCase {
                incoming_records: vec![test_json_tombstone('A')],
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
            TestCase {
                incoming_records: vec![
                    test_json_record('A'),
                    test_json_record('C'),
                    test_json_tombstone('B'),
                ],
                expected_record_count: 2,
                expected_tombstone_count: 1,
            },
        ];

        for tc in test_cases {
            log::info!("starting new testcase");
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
    fn test_change_local_guid() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;

        insert_local_record(&tx, test_record('C'))?;

        change_local_guid(
            &tx,
            &SyncGuid::new(&expand_test_guid('C')),
            &SyncGuid::new(&expand_test_guid('B')),
        )?;
        // XXX - TODO - check it's actually changed!
        Ok(())
    }

    #[test]
    fn test_get_incoming() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;

        insert_local_record(&tx, test_record('C'))?;
        save_incoming_records(&tx, vec![test_record('C')], &NeverInterrupts)?;

        let t = AddressesImpl {};
        t.fetch_incoming_states(&tx)?;

        // XXX - check we got what we expected!
        Ok(())
    }
}
