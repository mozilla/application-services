/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::error::*;
use crate::schema::json::FORMAT_VERSION;
use crate::schema::RecordSchema;
use crate::{Guid, JsonObject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use sync15_traits::ServerTimestamp;

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
    pub remerge_features: Vec<String>,
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
