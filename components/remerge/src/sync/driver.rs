use super::meta_records::{
    ClientInfos, MetaPayloads, RemoteSchemaEnvelope, SingleClientInfo, CLIENT_INFO_GUID,
    SCHEMA_GUID,
};
use super::records::RecordInfo;
use super::schema_action::{RemoteSchemaAction, UpgradeLocal, UpgradeRemote};
use crate::error::*;
use crate::storage::{meta, RemergeDb};
use sql_support::ConnExt;
use std::collections::HashMap;
use sync15_traits::{telemetry::Engine as Telem, *};
use sync15_traits::{CollSyncIds, StoreSyncAssociation};

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
    pub(crate) fn new(db: &'a mut RemergeDb) -> Self {
        // We write schema and clients metadata into the collection as records
        // with well-known ids.
        assert!(!db.info().native.legacy, "NYI: legacy schemas");
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
        new_timestamp: ServerTimestamp,
        mut records_synced: Vec<Guid>,
    ) -> Result<()> {
        records_synced.retain(|g| g != CLIENT_INFO_GUID && g != SCHEMA_GUID);
        self.db.mark_synchronized(new_timestamp, &records_synced)
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

    fn client_info(&self) -> Result<SingleClientInfo> {
        Ok(SingleClientInfo {
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
            let fresh = ClientInfos::from(self.client_info()?);
            self.outgoing.changes.push(Payload::from_record(fresh)?);
            return Ok(());
        };
        if when != ServerTimestamp(0) && when <= self.get_last_schema_fetch()? {
            return Ok(());
        }

        let mut scm = info.into_record::<ClientInfos>()?;
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
        let will_sync = match RemoteSchemaAction::determine(self.db.info(), &scm)? {
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

    pub(crate) fn get_sync_assoc(&self) -> Result<StoreSyncAssociation> {
        let global = self.try_get_meta::<Guid>(meta::GLOBAL_SYNCID_META_KEY)?;
        let coll = self.try_get_meta::<Guid>(meta::COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            StoreSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            StoreSyncAssociation::Disconnected
        })
    }

    pub(crate) fn reset(&self, assoc: &StoreSyncAssociation) -> Result<()> {
        match assoc {
            StoreSyncAssociation::Connected(CollSyncIds { global, coll }) => {
                self.put_meta(meta::GLOBAL_SYNCID_META_KEY, global)?;
                self.put_meta(meta::GLOBAL_SYNCID_META_KEY, coll)?;
            }
            StoreSyncAssociation::Disconnected => {
                self.del_meta(meta::GLOBAL_SYNCID_META_KEY)?;
                self.del_meta(meta::GLOBAL_SYNCID_META_KEY)?;
            }
        }
        self.del_meta(meta::LAST_SYNC_SERVER_MS)?;
        Ok(())
    }
    pub(crate) fn apply_incoming(
        &mut self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut Telem,
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
        let meta = MetaPayloads::from_changeset(inp)?;

        let (record, when) = meta.schema;
        self.process_schema_change(record, when)?;

        // This is probably not the right time to do these -- I s
        if let Some(upgrade_l) = self.local_upgrade.clone() {
            self.db.upgrade_local(upgrade_l.to)?;
        }
        if let Some(upgrade_r) = self.remote_upgrade.clone() {
            self.upgrade_remote(upgrade_r)?;
        }

        if !self.in_lockout && expect_len == 3 {
            let mut incoming_telemetry = telemetry::EngineIncoming::new();
            let mut records = iter.next().unwrap();
            records
                .changes
                .retain(|(record, _ts)| record.id != SCHEMA_GUID && record.id != CLIENT_INFO_GUID);
            let relevant =
                self.db
                    .fetch_for_sync(records.changes, &mut incoming_telemetry, &self.scope)?;
            self.reconcile(relevant, now, &mut incoming_telemetry)?;
            telem.incoming(incoming_telemetry);
            let out = self.db.fetch_outgoing()?;
            self.outgoing.changes.extend(out);
        }

        self.err_if_interrupted()?;
        let (record, when) = meta.clients;
        self.prepare_client_info_change(record, when)?;
        Ok(self.take_outgoing())
    }

    fn take_outgoing(&mut self) -> OutgoingChangeset {
        OutgoingChangeset {
            timestamp: self.outgoing.timestamp,
            collection: self.outgoing.collection.clone(),
            changes: std::mem::replace(&mut self.outgoing.changes, vec![]),
        }
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

    pub fn reconcile(
        &self,
        records: HashMap<Guid, RecordInfo>,
        server_now: ServerTimestamp,
        telem: &mut telemetry::EngineIncoming,
    ) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        for (_, mut record) in records {
            self.err_if_interrupted()?;
            log::debug!("Processing remote change {}", record.id);
            if record.inbound.0.deleted || record.inbound.0.payload.is_none() {
                log::debug!("  Deletion");
                self.db.sync_delete(record.id.clone())?;
                continue;
            }
            let (upstream, upstream_time) = record.inbound;
            // if upstream.vclock.is_ancestor_of()
            let _remote_age = server_now.duration_since(upstream_time).unwrap_or_default();

            match (record.mirror.take(), record.local.take()) {
                (Some(_mirror), Some(local)) => {
                    log::debug!("  Conflict between remote and local, Reconciling");

                    // match upstream.vclock.get_ordering(&local.vclock) {

                    let _local_age = std::time::SystemTime::now()
                        .duration_since(local.local_modified.into())
                        .unwrap_or_default();
                    let vc = local.vclock.combine(&upstream.vclock);

                    self.db.sync_mirror_update(upstream, vc, upstream_time)?;
                    //     self.db.sync_delete_local(local.id)?;
                    // }
                    // self.db.sync_update(upstream, upstream_time)?;
                    // plan.plan_three_way_merge(local, mirror, upstream, upstream_time, server_now);
                    telem.reconciled(1);
                }
                (Some(_mirror), None) => {
                    log::debug!("  Forwarding mirror to remote");
                    let vc = upstream.vclock.clone();
                    self.db.sync_mirror_update(upstream, vc, upstream_time)?;
                    telem.applied(1);
                }
                (None, Some(local)) => {
                    log::debug!("  Conflicting record without shared parent, using newer");
                    let vc = local.vclock.combine(&upstream.vclock);
                    self.db
                        .sync_mirror_insert(upstream, vc, upstream_time, true)?;
                    // (&local.login, (upstream, upstream_time));
                    telem.reconciled(1);
                }
                (None, None) => {
                    // if let Some(dupe) = self.find_dupe(&upstream)? {
                    //     log::debug!(
                    //         "  Incoming record {} was is a dupe of local record {}",
                    //         upstream.guid,
                    //         dupe.guid
                    //     );
                    //     plan.plan_two_way_merge(&dupe, (upstream, upstream_time));
                    // } else {
                    log::debug!("New record, inserting into mirror");
                    let vc = upstream.vclock.clone();
                    self.db
                        .sync_mirror_insert(upstream, vc, upstream_time, false)?;
                    // }
                    // telem.applied(1);
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

impl<'a> ConnExt for RemergeSync<'a> {
    fn conn(&self) -> &rusqlite::Connection {
        self.db.conn()
    }
}

// #[derive(Default, Debug, Clone)]
// pub(crate) struct UpdatePlan {
//     pub delete_mirror: Vec<Guid>,
//     pub delete_local: Vec<Guid>,
//     pub local_updates: Vec<(Rec)>,
//     // the bool is the `is_overridden` flag, the i64 is ServerTimestamp in millis
//     pub mirror_inserts: Vec<(Login, i64, bool)>,
//     pub mirror_updates: Vec<(Login, i64)>,
// }
