/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
mod meta_records;
pub(crate) mod schema_action;
mod store;
use crate::error::*;
use crate::storage::{meta, RemergeDb};
pub(crate) use meta_records::RemoteSchemaEnvelope;
use meta_records::{CLIENT_INFO_GUID, SCHEMA_GUID};
use schema_action::{RemoteSchemaAction, UpgradeLocal, UpgradeRemote};
use sql_support::ConnExt;
use sync15_traits::{telemetry::Engine as Telem, *};

pub struct RemergeSync<'a> {
    pub(crate) db: &'a mut RemergeDb,
    pub(crate) scope: sql_support::SqlInterruptScope,
    /// If true, we just do a sync of metadata
    in_lockout: bool,
    remote_upgrade: Option<UpgradeRemote>,
    local_upgrade: Option<UpgradeLocal>,
    outgoing: OutgoingChangeset,
    // local_records_for_sync: Vec<crate::storage::LocalRecord>,
}

impl<'a> RemergeSync<'a> {
    pub fn new(db: &'a mut RemergeDb) -> Self {
        let scope = db.begin_interrupt_scope();
        let in_lockout = db.in_sync_lockout().unwrap_or_default();
        let outgoing = OutgoingChangeset {
            collection: db.collection().to_owned().into(),
            changes: vec![],
            timestamp: ServerTimestamp::default(),
        };

        Self {
            db,
            scope,
            in_lockout,
            outgoing,
            remote_upgrade: None,
            local_upgrade: None,
        }
    }

    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        self.db.conn()
    }

    pub(crate) fn sync_finished(
        &mut self,
        _new_timestamp: ServerTimestamp,
        _records_synced: Vec<Guid>,
    ) -> Result<()> {
        unimplemented!();
    }

    fn get_last_sync(&self) -> Result<Option<ServerTimestamp>> {
        Ok(self
            .try_get_meta::<i64>(meta::LAST_SYNC_SERVER_MS)?
            .map(ServerTimestamp))
    }

    fn get_last_schema_fetch(&self) -> Result<ServerTimestamp> {
        let millis = self
            .try_get_meta::<i64>(meta::SCHEMA_FETCH_TIMESTAMP)?
            .unwrap_or_default();
        Ok(ServerTimestamp(millis))
    }

    fn err_if_interrupted(&self) -> Result<()> {
        if self.scope.was_interrupted() {
            Err(crate::error::ErrorKind::Interrupted.into())
        } else {
            Ok(())
        }
    }

    fn client_info(&self) -> Result<meta_records::SingleClientInfo> {
        Ok(meta_records::SingleClientInfo {
            id: self.db.client_id(),
            native_schema_version: self.db.info().native_schema().version.to_string(),
            local_schema_version: self.db.info().local_schema().version.to_string(),
            last_sync: self.get_last_sync()?,
            extra: Default::default(),
        })
    }

    fn prepare_client_info_change(
        &mut self,
        info: Option<Payload>,
        when: ServerTimestamp,
    ) -> Result<()> {
        let info = if let Some(p) = info {
            p
        } else {
            let fresh = meta_records::ClientInfos::from(self.client_info()?);
            self.outgoing.changes.push(Payload::from_record(fresh)?);
            return Ok(());
        };
        if when != ServerTimestamp(0) && when <= self.get_last_schema_fetch()? {
            return Ok(());
        }

        let mut scm = info.into_record::<meta_records::ClientInfos>()?;
        let info = self.client_info()?;
        scm.clients.insert(info.id.clone(), info);
        self.outgoing.changes.push(Payload::from_record(scm)?);
        self.err_if_interrupted()?;
        Ok(())
    }

    fn upgrade_remote(&mut self, up: UpgradeRemote) -> Result<()> {
        self.db.upgrade_remote(&up)?;
        let schema = RemoteSchemaEnvelope::new(&self.db.info().local, self.db.client_id());
        self.outgoing.changes.push(Payload::from_record(schema)?);
        Ok(())
    }

    fn process_schema_change(
        &mut self,
        schema: Option<Payload>,
        when: ServerTimestamp,
    ) -> Result<()> {
        let schema = if let Some(s) = schema {
            s
        } else {
            self.exit_lockout()?;
            // Avoid borrow issues
            self.remote_upgrade = Some(UpgradeRemote {
                from: None,
                fresh_server: true,
            });
            return Ok(());
        };
        if when != ServerTimestamp(0) && when <= self.get_last_schema_fetch()? {
            return Ok(());
        }
        // XXX Consider error handling here!
        let scm = schema.into_record::<RemoteSchemaEnvelope>()?;
        let will_sync = match schema_action::determine_action(self.db.info(), &scm)? {
            RemoteSchemaAction::UpgradeRemote(up_rem) => {
                self.remote_upgrade = Some(up_rem);
                true
            }
            RemoteSchemaAction::UpgradeLocal(up_local) => {
                self.local_upgrade = Some(up_local);
                true
            }
            RemoteSchemaAction::SyncNormally => true,
            RemoteSchemaAction::LockedOut => false,
        };
        if !will_sync {
            self.enter_lockout(scm.get_version_req().ok())?;
        } else {
            self.exit_lockout()?;
        }
        self.err_if_interrupted()?;
        Ok(())
    }
    fn enter_lockout(&mut self, v: Option<semver::VersionReq>) -> Result<()> {
        self.in_lockout = true;
        if let Some(v) = v {
            self.put_meta(meta::SYNC_NATIVE_VERSION_THRESHOLD, &v.to_string())?;
        }
        Ok(())
    }

    fn exit_lockout(&mut self) -> Result<()> {
        self.in_lockout = false;
        self.del_meta(meta::SYNC_NATIVE_VERSION_THRESHOLD)
    }

    pub(crate) fn get_collection_requests(&mut self) -> Result<Vec<CollectionRequest>> {
        let mut reqs = vec![CollectionRequest::new(self.db.collection().to_owned())
            .full()
            .ids(&[CLIENT_INFO_GUID, SCHEMA_GUID])];

        if !self.in_lockout {
            reqs.push(
                CollectionRequest::new(self.db.collection().to_owned())
                    .full()
                    .newer_than(self.get_last_sync()?.unwrap_or_default()),
            );
        }
        Ok(reqs)
    }

    pub(crate) fn apply_incoming(
        &mut self,
        inbound: Vec<IncomingChangeset>,
        _telem: &mut Telem,
    ) -> Result<OutgoingChangeset> {
        self.err_if_interrupted()?;
        let expect_len = if self.in_lockout { 1 } else { 2 };
        ensure!(
            inbound.len() == expect_len,
            format!(
                "Got wrong number of inbound changesets in apply_incoming. Want {}, got {}",
                expect_len,
                inbound.len()
            )
        );
        let mut iter = inbound.into_iter();
        let inp = iter.next().unwrap();
        let now = inp.timestamp;
        self.outgoing.timestamp = now;
        self.err_if_interrupted()?;
        let meta = meta_records::MetaPayloads::from_changeset(inp)?;

        let (record, when) = meta.schema;
        self.process_schema_change(record, when)?;

        // This is probably not the right time to do these -- I s
        if let Some(upgrade_l) = self.local_upgrade.clone() {
            self.db.upgrade_local(upgrade_l.to)?;
        }
        if let Some(upgrade_r) = self.remote_upgrade.clone() {
            self.upgrade_remote(upgrade_r)?;
        }

        if expect_len == 3 {
            let _records = iter.next().unwrap();
            unimplemented!();
            // self.apply_records(records)?;
        }

        self.err_if_interrupted()?;
        let (record, when) = meta.clients;
        self.prepare_client_info_change(record, when)?;

        unimplemented!();
    }

    pub(crate) fn put_meta(&self, key: meta::MetaKey, value: &dyn rusqlite::ToSql) -> Result<()> {
        meta::put(self.conn(), key, value)
    }

    pub(crate) fn del_meta(&self, key: meta::MetaKey) -> Result<()> {
        meta::delete(self.conn(), key)
    }
    pub(crate) fn try_get_meta<T: rusqlite::types::FromSql>(
        &self,
        key: meta::MetaKey,
    ) -> Result<Option<T>> {
        meta::try_get(self.conn(), key)
    }
}

impl<'a> ConnExt for RemergeSync<'a> {
    fn conn(&self) -> &rusqlite::Connection {
        self.db.conn()
    }
}
