/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::AddressRecord;
use crate::db::schema::{ADDRESS_COMMON_COLS, ADDRESS_COMMON_VALS};
use crate::error::*;
use crate::sync::common::*;
use crate::sync::{IncomingRecords, MergeResult, RecordStorageImpl, SyncRecord};
use crate::sync_merge_field_check;
use interrupt_support::Interruptee;
use rusqlite::{named_params, types::ToSql, Connection, Transaction};
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

type IncomingState = crate::sync::IncomingState<AddressRecord>;

/// Incoming tombstones are retrieved from the `addresses_tombstone_sync_staging` table
/// and assigned `IncomingState` values.
fn get_incoming_tombstone_states(conn: &Connection) -> Result<Vec<IncomingState>> {
    let sql = "SELECT
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
    LEFT JOIN addresses_tombstones t ON s.guid = t.guid";

    common_get_incoming_tombstone_states(conn, sql)
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

    common_get_incoming_record_states(conn, sql_query)
}

pub(super) struct AddressesImpl<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> AddressesImpl<'a> {
    pub fn new(tx: &'a Transaction<'a>) -> Self {
        Self { tx }
    }
}

impl<'a> RecordStorageImpl for AddressesImpl<'a> {
    type Record = AddressRecord;

    /// The first step in the "apply incoming" process - stage the records
    fn stage_incoming(
        &self,
        incoming: IncomingRecords<Self::Record>,
        signal: &dyn Interruptee,
    ) -> Result<()> {
        // The non-tombstone records...
        common_stage_incoming_records(
            self.tx,
            "addresses_sync_staging",
            ADDRESS_COMMON_COLS,
            incoming.records,
            signal,
            |record| {
                vec![
                    &record.guid as &dyn ToSql,
                    &record.given_name,
                    &record.additional_name,
                    &record.family_name,
                    &record.organization,
                    &record.street_address,
                    &record.address_level3,
                    &record.address_level2,
                    &record.address_level1,
                    &record.postal_code,
                    &record.country,
                    &record.tel,
                    &record.email,
                    &record.metadata.time_created,
                    &record.metadata.time_last_used,
                    &record.metadata.time_last_modified,
                    &record.metadata.times_used,
                ]
            },
        )?;
        // and tombstones
        common_stage_incoming_tombstones(
            self.tx,
            "addresses_tombstone_sync_staging",
            incoming.tombstones,
            signal,
        )?;
        Ok(())
    }

    /// The second step in the "apply incoming" process for syncing autofill address records.
    /// Incoming tombstones and records are retrieved from the temp tables and assigned
    /// `IncomingState` values.
    fn fetch_incoming_states(&self) -> Result<Vec<IncomingState>> {
        let mut incoming_infos = get_incoming_tombstone_states(self.tx)?;
        let mut incoming_record_infos = get_incoming_record_states(self.tx)?;
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

        sync_merge_field_check!(given_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(additional_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(family_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(organization, incoming, local, mirror, merged_record);
        sync_merge_field_check!(street_address, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level3, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level2, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level1, incoming, local, mirror, merged_record);
        sync_merge_field_check!(postal_code, incoming, local, mirror, merged_record);
        sync_merge_field_check!(country, incoming, local, mirror, merged_record);
        sync_merge_field_check!(tel, incoming, local, mirror, merged_record);
        sync_merge_field_check!(email, incoming, local, mirror, merged_record);

        merged_record.metadata = incoming.metadata;
        merged_record
            .metadata
            .merge(&local.metadata, &mirror.as_ref().map(|m| m.metadata()));

        MergeResult::Merged {
            merged: merged_record,
        }
    }

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(&self, incoming: &Self::Record) -> Result<Option<(SyncGuid, Self::Record)>> {
        let sql = format!("
            SELECT
                {common_cols},
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
                AND email == :email", common_cols = ADDRESS_COMMON_COLS);

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

        let result = self.tx.query_row_named(&sql, params, |row| {
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

    fn update_local_record(&self, new_record: AddressRecord, flag_as_changed: bool) -> Result<()> {
        let rows_changed = self.tx.execute_named(
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
                ":time_created": new_record.metadata.time_created,
                ":time_last_used": new_record.metadata.time_last_used,
                ":time_last_modified": new_record.metadata.time_last_modified,
                ":times_used": new_record.metadata.times_used,
                ":guid": new_record.guid,
                ":change_counter_incr": flag_as_changed as u32,
            },
        )?;
        // if we didn't actually update a row them something has gone very wrong...
        assert_eq!(rows_changed, 1);
        Ok(())
    }

    fn insert_local_record(&self, new_record: AddressRecord) -> Result<()> {
        self.tx.execute_named(
            &format!(
                "INSERT OR IGNORE INTO addresses_data (
                {common_cols},
                sync_change_counter
            ) VALUES (
                {common_vals},
                :sync_change_counter
            )",
                common_cols = ADDRESS_COMMON_COLS,
                common_vals = ADDRESS_COMMON_VALS
            ),
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
                ":time_created": new_record.metadata.time_created,
                ":time_last_used": new_record.metadata.time_last_used,
                ":time_last_modified": new_record.metadata.time_last_modified,
                ":times_used": new_record.metadata.times_used,
                ":sync_change_counter": 0,
            },
        )?;

        Ok(())
    }

    /// Changes the guid of the local record for the given `old_guid` to the given `new_guid` used
    /// for the `HasLocalDupe` incoming state, and mark the item as dirty.
    fn change_local_guid(&self, old_guid: &SyncGuid, new_guid: &SyncGuid) -> Result<()> {
        common_change_guid(self.tx, "addresses_data", old_guid, new_guid)
    }

    fn remove_record(&self, guid: &SyncGuid) -> Result<()> {
        common_remove_record(self.tx, "addresses_data", guid)
    }

    fn remove_tombstone(&self, guid: &SyncGuid) -> Result<()> {
        common_remove_record(self.tx, "addresses_tombstones", guid)
    }
}

/// Returns a with the given local record's data but with a new guid and
/// fresh sync metadata.
fn get_forked_record(local_record: AddressRecord) -> AddressRecord {
    let mut local_record_data = local_record;
    local_record_data.guid = SyncGuid::random();
    local_record_data.metadata.time_created = Timestamp::now();
    local_record_data.metadata.time_last_used = Timestamp::now();
    local_record_data.metadata.time_last_modified = Timestamp::now();
    local_record_data.metadata.times_used = 0;
    local_record_data.metadata.sync_change_counter = Some(1);

    local_record_data
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;
    use crate::db::addresses::get_address;
    use crate::sync::common::tests::*;

    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Map, Value};
    use sql_support::ConnExt;

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

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = new_syncable_mem_db();
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
            let tx = db.transaction()?;
            let ri = AddressesImpl::new(&tx);
            ri.stage_incoming(array_to_incoming(tc.incoming_records), &NeverInterrupts)?;

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
        let ri = AddressesImpl::new(&tx);

        ri.insert_local_record(test_record('C'))?;

        ri.change_local_guid(
            &SyncGuid::new(&expand_test_guid('C')),
            &SyncGuid::new(&expand_test_guid('B')),
        )?;
        tx.commit()?;
        assert!(get_address(&db.writer, &expand_test_guid('C').into()).is_err());
        assert!(get_address(&db.writer, &expand_test_guid('B').into()).is_ok());
        Ok(())
    }

    #[test]
    fn test_get_incoming() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ai = AddressesImpl::new(&tx);
        do_test_incoming_same(&ai, test_record('C'));
    }
}
