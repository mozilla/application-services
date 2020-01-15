/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::error::*;
use crate::schema::json::FORMAT_VERSION;
use crate::schema::RecordSchema;
use crate::{Guid, JsonObject, Sym};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use sync15_traits::{IncomingChangeset, Payload, ServerTimestamp};

pub const SCHEMA_GUID: &str = "__schema__";
pub const CLIENT_INFO_GUID: &str = "__client_info__";

#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteSchemaEnvelope {
    pub id: Guid, // Always SCHEMA_GUID
    pub uploader_id: Guid,
    pub schema_text: Arc<str>,
    pub schema_version: semver::Version,
    #[serde(default)]
    pub required_version: Option<semver::VersionReq>,
    pub format_version: i64,
    #[serde(default)]
    pub remerge_features: Vec<Sym>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

impl RemoteSchemaEnvelope {
    pub fn new(r: &RecordSchema, local_id: Guid) -> Self {
        Self {
            id: SCHEMA_GUID.into(),
            schema_text: r.source.clone(),
            schema_version: r.version.clone(),
            required_version: Some(r.required_version.clone()),
            format_version: FORMAT_VERSION,
            remerge_features: r.remerge_features_used.clone(),
            uploader_id: local_id,
            extra: JsonObject::default(),
        }
    }
}

impl RemoteSchemaEnvelope {
    pub(crate) fn get_version_req(&self) -> Result<semver::VersionReq> {
        let reqv = self
            .required_version
            .clone()
            .map(Ok)
            .unwrap_or_else(|| crate::util::compatible_version_req(&self.schema_version))
            .map_err(|e| format!("Bad version {:?}: {}", self.schema_version, e))?;
        Ok(reqv)
    }
    pub(crate) fn uses_future_features(&self) -> bool {
        use crate::schema::desc::REMERGE_FEATURES_UNDERSTOOD;
        self.remerge_features
            .iter()
            .any(|f| !REMERGE_FEATURES_UNDERSTOOD.contains(&f.as_str()))
    }
}

#[derive(Serialize, Deserialize)]
pub struct ClientInfos {
    pub id: Guid, // Always CLIENT_INFO_GUID
    pub clients: BTreeMap<Guid, SingleClientInfo>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

impl From<SingleClientInfo> for ClientInfos {
    #[inline]
    fn from(src: SingleClientInfo) -> Self {
        Self {
            id: CLIENT_INFO_GUID.into(),
            clients: std::iter::once((src.id.clone(), src)).collect(),
            extra: Default::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SingleClientInfo {
    pub id: Guid,
    pub native_schema_version: String,
    pub local_schema_version: String,
    #[serde(default)]
    pub last_sync: Option<ServerTimestamp>,
    // Be sure to round-trip data from other clients.
    #[serde(flatten)]
    pub extra: JsonObject,
}

pub(super) struct MetaPayloads {
    pub(super) clients: (Option<Payload>, ServerTimestamp),
    pub(super) schema: (Option<Payload>, ServerTimestamp),
}

impl MetaPayloads {
    pub(super) fn from_changeset(m: IncomingChangeset) -> Result<Self> {
        let changes = m.changes.len();
        let mut it = m.changes.into_iter();
        Ok(match changes {
            0 => Self::empty_at(m.timestamp),
            1 => {
                let c = it.next().unwrap();
                if c.0.id == SCHEMA_GUID {
                    Self::schema_at(c, m.timestamp)
                } else {
                    Self::client_at(c, m.timestamp)
                }
            }
            2 => {
                let a = it.next().unwrap();
                let b = it.next().unwrap();
                if a.0.id == CLIENT_INFO_GUID {
                    Self::from_clients_and_schema_at(a, b, m.timestamp)
                } else {
                    Self::from_clients_and_schema_at(b, a, m.timestamp)
                }
            }
            n => {
                throw_msg!("Requested only 2 metadata records, got: {}", n);
            }
        })
    }
    fn from_clients_and_schema_at(
        clients: impl Into<Option<(Payload, ServerTimestamp)>>,
        schema: impl Into<Option<(Payload, ServerTimestamp)>>,
        time: ServerTimestamp,
    ) -> Self {
        let clients = clients.into();
        if let Some(v) = &clients {
            debug_assert_eq!(v.0.id, CLIENT_INFO_GUID);
        }
        let schema = schema.into();
        if let Some(v) = &schema {
            debug_assert_eq!(v.0.id, SCHEMA_GUID);
        }
        Self {
            clients: ensure_dated(clients, time),
            schema: ensure_dated(schema, time),
        }
    }

    fn empty_at(time: ServerTimestamp) -> Self {
        Self::from_clients_and_schema_at(None, None, time)
    }

    fn schema_at(v: impl Into<Option<(Payload, ServerTimestamp)>>, time: ServerTimestamp) -> Self {
        Self::from_clients_and_schema_at(None, v, time)
    }

    fn client_at(v: impl Into<Option<(Payload, ServerTimestamp)>>, time: ServerTimestamp) -> Self {
        Self::from_clients_and_schema_at(v, None, time)
    }
}

fn ensure_dated(
    meta: Option<(Payload, ServerTimestamp)>,
    time: ServerTimestamp,
) -> (Option<Payload>, ServerTimestamp) {
    meta.map(|(p, t)| (Some(p), t))
        .unwrap_or_else(|| (None, time))
}
