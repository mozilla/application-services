/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::RemoteSchemaEnvelope;
use crate::error::*;
use crate::storage::SchemaBundle;
use crate::RecordSchema;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteSchemaAction {
    /// Sync normally without replacing any schemas.
    SyncNormally,
    /// Our native version is locked out. We stop syncing, and hopefully
    /// eventually get an update.
    LockedOut,
    /// It's old, replace with our local schema.
    UpgradeRemote(Arc<RecordSchema>),
    /// We're old, replace our local schema with this.
    UpgradeLocal(Arc<RecordSchema>),
}

pub fn determine_action(
    bundle: &SchemaBundle,
    remote: &RemoteSchemaEnvelope,
) -> Result<RemoteSchemaAction> {
    let reqv = remote.get_version_req()?;
    if !reqv.matches(&remote.schema_version) {
        // Bail out, hope it ends up in telemetry. Probably a mistake by the schema author.
        throw_msg!(
            "Invalid remote schema versions: schema_version = {:?}, requires = {:?}",
            remote.schema_version,
            reqv
        );
    }
    let native_v = &bundle.native_schema().version;
    let local_v = &bundle.local_schema().version;
    if !reqv.matches(native_v) {
        log::warn!(
            "Schema version req {:?} does not match native version {:?} (local {:?}). Locked out.",
            reqv,
            native_v,
            local_v
        );
        return Ok(RemoteSchemaAction::LockedOut);
    }
    if local_v == &remote.schema_version {
        log::info!("Local and remote schemas are the same");
        return Ok(RemoteSchemaAction::SyncNormally);
    }

    if remote.format_version > crate::schema::json::FORMAT_VERSION || remote.uses_future_features()
    {
        throw_msg!("Schema is from future version and can't be understood. Version {} should have been locked out.", native_v);
    } else if local_v > &remote.schema_version {
        log::info!(
            "Remote version ({:?}) is lower than our native versions ({:?}). Requires update",
            remote.schema_version,
            native_v
        );
        Ok(RemoteSchemaAction::UpgradeRemote(
            bundle.local_schema().clone(),
        ))
    } else if &remote.schema_version > local_v {
        let schema = crate::schema::parse_from_string(remote.schema_text.clone(), true);
        match schema {
            Ok(v) => Ok(RemoteSchemaAction::UpgradeLocal(v.into())),
            Err(e) => {
                log::error!("Can't read future schema {:?}", e);
                // Hrm...
                return Err(e.into());
            }
        }
    } else {
        Ok(RemoteSchemaAction::SyncNormally)
    }
}
