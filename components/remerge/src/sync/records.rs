/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
pub use crate::storage::RawRecord;
use crate::Guid;
use crate::MsTime;
use crate::VClock;
use serde::{Deserialize, Serialize};
use sync15_traits::ServerTimestamp;

/// TODO: This is only for non-`legacy` collections! For legacy collections it's
/// totally wrong.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RemoteRecord {
    pub id: Guid,
    pub vclock: VClock,
    pub last_writer: Guid,
    pub schema_version: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "crate::util::is_default")]
    pub deleted: bool,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<RawRecord>,
    // TODO: include a timestamp in here?
}
#[derive(Clone, Debug)]
pub struct MirrorRecord {
    pub id: Guid,
    // None if tombstone. String is schema version
    pub inner: Option<(RawRecord, String)>,
    pub server_modified: ServerTimestamp,
    pub vclock: VClock,
    pub last_writer: Guid,
    pub is_overridden: bool,
}

#[derive(Clone, Debug)]
pub struct LocalRecord {
    pub id: Guid,
    // None if tombstone. String is schema version
    pub inner: Option<(RawRecord, String)>,
    pub local_modified: MsTime,
    pub last_writer: Guid,
    pub vclock: VClock,
}
#[derive(Clone)]
pub struct RecordInfo {
    pub id: Guid,
    pub local: Option<LocalRecord>,
    pub inbound: (RemoteRecord, ServerTimestamp),
    pub mirror: Option<MirrorRecord>,
}

impl RecordInfo {
    pub fn new(remote: RemoteRecord, ts: ServerTimestamp) -> Self {
        Self {
            id: remote.id.clone(),
            local: None,
            mirror: None,
            inbound: (remote, ts),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WriteMeta {
    pub vclock: VClock,
    pub writer: Guid,
    pub time: i64,
    pub schema: String,
}
