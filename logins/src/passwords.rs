// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

use chrono::{
    TimeZone,
};

use mentat::{
    Binding,
    DateTime,
    Entid,
    QueryInputs,
    QueryResults,
    Queryable,
    TxReport,
    TypedValue,
    Utc,
};

use mentat::conn::{
    InProgress,
};

use mentat::entity_builder::{
    BuildTerms,
    TermBuilder,
};

use credentials::{
    self,
    build_credential,
    find_credential_by_content,
};
use errors::{
    Error,
    Result,
};
use types::{
    Credential,
    CredentialId,
    FormTarget,
    ServerPassword,
    SyncGuid,
};

use vocab::{
    CREDENTIAL_ID,
    CREDENTIAL_PASSWORD,
    CREDENTIAL_USERNAME,
    FORM_HOSTNAME,
    FORM_HTTP_REALM,
    FORM_PASSWORD_FIELD,
    FORM_SUBMIT_URL,
    FORM_SYNC_PASSWORD,
    FORM_USERNAME_FIELD,
    SYNC_PASSWORD_CREDENTIAL,
    SYNC_PASSWORD_MATERIAL_TX,
    SYNC_PASSWORD_METADATA_TX,
    SYNC_PASSWORD_SERVER_MODIFIED,
    SYNC_PASSWORD_TIMES_USED,
    SYNC_PASSWORD_TIME_CREATED,
    SYNC_PASSWORD_TIME_LAST_USED,
    SYNC_PASSWORD_TIME_PASSWORD_CHANGED,
    SYNC_PASSWORD_UUID,
};

/// Fetch the Sync 1.5 password with given `uuid`.
pub fn get_sync_password<Q>(queryable: &Q, uuid: SyncGuid) -> Result<Option<ServerPassword>>
where Q: Queryable {
    let q = r#"[:find
                [(pull ?c [:credential/id :credential/username :credential/password :credential/createdAt])
                 (pull ?f [:form/hostname :form/usernameField :form/passwordField :form/submitUrl :form/httpRealm])]
                :in
                ?uuid
                :where
                [?sp :sync.password/uuid ?uuid]
                [?sp :sync.password/credential ?c]
                [?c :credential/id _] ; Deleted credentials produce dangling :sync.password/credential refs; ignore them.
                [?f :form/syncPassword ?sp]
               ]"#;

    let inputs = QueryInputs::with_value_sequence(vec![
        (var!(?uuid), TypedValue::typed_string(&uuid)),
    ]);

    let server_password = match queryable.q_once(q, inputs)?.into_tuple()? {
        Some((Binding::Map(cm), Binding::Map(fm))) => {
            let credential = Credential::from_structured_map(cm.as_ref()).ok_or_else(|| Error::BadQueryResultType)?;

            // TODO: improve error handling and messaging throughout.
            let hostname = fm.0.get(&*FORM_HOSTNAME).and_then(|x| x.as_string()).map(|x| (**x).clone()).unwrap();
            let username_field = fm.0.get(&*FORM_USERNAME_FIELD).and_then(|x| x.as_string()).map(|x| (**x).clone());
            let password_field = fm.0.get(&*FORM_PASSWORD_FIELD).and_then(|x| x.as_string()).map(|x| (**x).clone());

            let form_submit_url = fm.0.get(&*FORM_SUBMIT_URL).and_then(|x| x.as_string()).map(|x| (**x).clone());
            let http_realm = fm.0.get(&*FORM_HTTP_REALM).and_then(|x| x.as_string()).map(|x| (**x).clone());

            // TODO: produce a more informative error in this situation.
            let target = match (form_submit_url, http_realm) {
                // Logins with both a formSubmitURL and httpRealm are not valid.
                (Some(_), Some(_)) => bail!(Error::BadQueryResultType),
                (Some(form_submit_url), _) => FormTarget::FormSubmitURL(form_submit_url),
                (_, Some(http_realm)) => FormTarget::HttpRealm(http_realm),
                // Login must have at least a formSubmitURL or httpRealm.
                _ => bail!(Error::BadQueryResultType),
            };

            Ok(Some(ServerPassword {
                modified: time_sync_password_modified(queryable, uuid.clone())?.expect("time_sync_password_modified"),
                uuid: uuid.clone(),
                hostname: hostname,
                target: target,
                username: credential.username,
                password: credential.password,
                username_field,
                password_field,
                time_created: credential.created_at,
                time_password_changed: time_password_changed(queryable, uuid.clone())?.expect("time_password_changed"),
                time_last_used: time_last_used(queryable, uuid.clone())?,
                times_used: times_used(queryable, uuid.clone())? as usize,
            }))
        },
        None => Ok(None),
        _ => bail!(Error::BadQueryResultType),
    };

    server_password
}

/// Fetch all known Sync 1.5 passwords.
///
/// No ordering is implied.
pub fn get_all_sync_passwords<Q>(queryable: &Q) -> Result<Vec<ServerPassword>>
where Q: Queryable {
    let q = r#"[
:find
 [?uuid ...]
:where
 [_ :sync.password/uuid ?uuid]
:order
 (asc ?uuid) ; We order for testing convenience.
]"#;

    let uuids: Result<Vec<_>> = queryable.q_once(q, None)?
        .into_coll()?
        .into_iter()
        .map(|uuid| {
            match uuid {
                Binding::Scalar(TypedValue::String(uuid)) => Ok(SyncGuid((*uuid).clone())),
                _ => bail!(Error::BadQueryResultType),
            }
        })
        .collect();
    let uuids = uuids?;

    // TODO: do this more efficiently.
    let mut ps = Vec::with_capacity(uuids.len());

    for uuid in uuids {
        get_sync_password(queryable, uuid)?.map(|p| ps.push(p));
    }

    Ok(ps)
}

/// Fetch all credential IDs that do not correspond to existing Sync 1.5 passwords.
///
/// Such credentials need to be associated with a Sync 1.5 password and a corresponding record
/// uploaded to the service (although as yet we don't implement this).
///
/// No ordering is implied.
pub fn get_new_credential_ids<Q>(queryable: &Q) -> Result<Vec<CredentialId>>
where Q: Queryable
{
    // TODO: narrow by tx?  We only care about credentials created after the last sync tx; if we
    // index on creation we can find just those credentials more efficiently.
    let q = r#"[:find
                [?id ...]
                :where
                [?c :credential/id ?id]
                (not [_ :sync.password/credential ?c])
                :order ; TODO: don't order?
                ?id
               ]"#;

    let vs = queryable.q_once(q, None)?.into_coll()?;
    let new_ids: Result<Vec<_>> = vs.into_iter()
        .map(|id| match id {
            Binding::Scalar(TypedValue::String(id)) => Ok(CredentialId((*id).clone())),
            _ => bail!(Error::BadQueryResultType),
        })
        .collect();
    new_ids
}

/// Fetch all Sync 1.5 password uuids that have been deleted locally.
///
/// Such deleted password records either don't have a linked credential, or have a dangling
/// reference to a credential with no id.
pub fn get_deleted_sync_password_uuids_to_upload<Q>(queryable: &Q) -> Result<Vec<SyncGuid>>
where Q: Queryable
{
    // TODO: is there a way to narrow by tx?  Probably not here, since we're talking about records
    // that have been removed.  We could walk tx-data instead of searching datoms; who knows if that
    // is faster in practice.
    let q = r#"[:find
                [?uuid ...]
                :where
                [?sl :sync.password/uuid ?uuid]
                (not-join [?sl] [?sl :sync.password/credential ?credential] [?credential :credential/id _])
                :order ; TODO: don't order?
                ?uuid
               ]"#;

    let vs = queryable.q_once(q, None)?.into_coll()?;
    let deleted_uuids: Result<Vec<_>> = vs.into_iter()
        .map(|id| match id {
            Binding::Scalar(TypedValue::String(id)) => Ok(SyncGuid((*id).clone())),
            _ => bail!(Error::BadQueryResultType),
        })
        .collect();
    deleted_uuids
}

/// Delete the underlying credential, any logins, and the Sync 1.5 password record with the given
/// `uuid`, if such a record exists.
pub fn delete_by_sync_uuid(in_progress: &mut InProgress, uuid: SyncGuid) -> Result<()> {
    delete_by_sync_uuids(in_progress, vec![uuid])
}

/// Delete the underlying credentials, any logins, and the Sync 1.5 password records with the given
/// `uuids`, if any such records exist.
pub fn delete_by_sync_uuids(in_progress: &mut InProgress, uuids: Vec<SyncGuid>) -> Result<()> {
    // TODO: use `:db/retractEntity` to make this less onerous and avoid cloning.
    let q = r#"[
        :find
         ?e ?a ?v
        :in
         ?uuid
        :where
         (or-join [?e ?a ?v ?uuid]
          (and
           [?e :sync.password/uuid ?uuid]
           [?e ?a ?v])
          (and
           [?p :sync.password/uuid ?uuid]
           [?p :sync.password/credential ?e]
           [?e ?a ?v])
          (and
           [?p :sync.password/uuid ?uuid]
           [?p :sync.password/credential ?c]
           [?e :login/credential ?c]
           [?e ?a ?v])
          (and
           [?p :sync.password/uuid ?uuid]
           [?e :form/syncPassword ?p]
           [?e ?a ?v]))
        ]"#;

    let mut builder = TermBuilder::new();

    // TODO: do this in one query.  It's awkward because Mentat doesn't support binding non-scalar
    // inputs yet; see https://github.com/mozilla/mentat/issues/714.
    for uuid in uuids {
        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(uuid))]);
        let results = in_progress.q_once(q, inputs)?.results;

        match results {
            QueryResults::Rel(vals) => {
                // TODO: implement tuple binding for rel result rows.
                for vs in vals {
                    match (vs.len(), vs.get(0), vs.get(1), vs.get(2)) {
                        (3, Some(&Binding::Scalar(TypedValue::Ref(e))), Some(&Binding::Scalar(TypedValue::Ref(a))), Some(&Binding::Scalar(ref v))) => {
                            builder.retract(e, a, v.clone())?; // TODO: don't clone.
                        }
                        _ => bail!(Error::BadQueryResultType),
                    }
                }
            },
            _ => bail!(Error::BadQueryResultType),
        }
    }

    in_progress.transact_builder(builder).map_err(|e| e.into()).and(Ok(()))
}

/// Fetch Sync 1.5 passwords that have been modified locally and need to have fresh records uploaded
/// to the service.
///
/// No ordering is implied.
pub fn get_modified_sync_passwords_to_upload<Q>(queryable: &Q) -> Result<Vec<ServerPassword>>
where Q: Queryable
{
    let q = r#"[:find
            ;(max ?txI) ; Useful for debugging.
            [?uuid ...]
            :order
            ?uuid ; TODO: don't order?
            :with
            ?sp
            :where
            [?sp :sync.password/uuid ?uuid]
            [?sp :sync.password/materialTx ?materialTx]

            (or-join [?sp ?a ?tx]
             (and
              [?sp :sync.password/credential ?c]
              [?c ?a _ ?tx]
              [(ground [:credential/id :credential/username :credential/password]) [?a ...]])
             (and
              [?f :form/syncPassword ?sp]
              [?f ?a _ ?tx]
              [(ground [:form/hostname :form/usernameField :form/passwordField :form/submitUrl :form/httpRealm]) [?a ...]]))

            [(tx-after ?tx ?materialTx)]
           ;[?tx :db/txInstant ?txI] ; Useful for debugging.
           ]"#;

    let uuids: Result<Vec<_>> = queryable.q_once(q, None)?
        .into_coll()?
        .into_iter()
        .map(|uuid| {
            match uuid {
                Binding::Scalar(TypedValue::String(uuid)) => Ok(SyncGuid((*uuid).clone())),
                _ => bail!(Error::BadQueryResultType),
            }
        })
        .collect();
    let uuids = uuids?;

    let mut ps = Vec::with_capacity(uuids.len());

    for uuid in uuids {
        get_sync_password(queryable, uuid)?.map(|p| ps.push(p));
    }

    Ok(ps)
}

/// Mark the Sync 1.5 passwords with the given `uuids` as last synced at the given `tx_id`.
pub fn mark_synced_by_sync_uuids(in_progress: &mut InProgress, uuids: Vec<SyncGuid>, tx_id: Entid) -> Result<()> {
    let q = r#"[
        :find
         ?e .
        :in
         ?uuid
        :where
         [?e :sync.password/uuid ?uuid]
        ]"#;

    let tx = TypedValue::Ref(tx_id);

    let mut builder = TermBuilder::new();

    // TODO: do this in one query (or transaction).  It's awkward because Mentat doesn't support
    // binding non-scalar inputs yet; see https://github.com/mozilla/mentat/issues/714.
    for uuid in uuids {
        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(uuid))]);
        match in_progress.q_once(q, inputs)?.results {
            QueryResults::Scalar(Some(Binding::Scalar(TypedValue::Ref(e)))) => {
                builder.add(e, SYNC_PASSWORD_MATERIAL_TX.clone(), tx.clone())?;
                builder.add(e, SYNC_PASSWORD_METADATA_TX.clone(), tx.clone())?;
            },
            _ => bail!(Error::BadQueryResultType),
        }
    }

    in_progress.transact_builder(builder).map_err(|e| e.into()).and(Ok(()))
}

/// Mark all known Sync 1.5 passwords as having never been synced.
///
/// After this reset, every Sync 1.5 password record will be considered modified (locally).
pub fn reset_client(in_progress: &mut InProgress) -> Result<()> {
    let q = r#"[
:find
 [?e ...]
:where
 [?e :sync.password/uuid _]
]"#;

    // TODO: use a valid transaction ID.
    let tx = TypedValue::Ref(0);

    let mut builder = TermBuilder::new();

    let results = in_progress.q_once(q, None)?.results;

    match results {
        QueryResults::Coll(es) => {
            for e in es {
                match e {
                    Binding::Scalar(TypedValue::Ref(e)) => {
                        builder.add(e, SYNC_PASSWORD_MATERIAL_TX.clone(), tx.clone())?;
                        builder.add(e, SYNC_PASSWORD_METADATA_TX.clone(), tx.clone())?;
                    },
                    _ => bail!(Error::BadQueryResultType),
                }
            }
        },
        _ => bail!(Error::BadQueryResultType),
    }

    in_progress.transact_builder(builder).map_err(|e| e.into()).and(Ok(()))
}

/// Return the credential ID associated to the given Sync 1.5 password `uuid`, or `None` if no such
/// Sync 1.5 password exists.
fn find_credential_id_by_sync_password_uuid<Q>(queryable: &Q, uuid: SyncGuid) -> Result<Option<CredentialId>>
where Q: Queryable
{
    let q = r#"[:find ?id .
                :in
                ?uuid
                :where
                [?c :credential/id ?id]
                [?l :sync.password/credential ?c]
                [?l :sync.password/uuid ?uuid]]"#;

    let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(uuid))]);

    match queryable.q_once(q, inputs)?.into_scalar()? {
        Some(x) => {
            match x.into_string() {
                Some(x) => Ok(Some(CredentialId((*x).clone()))),
                None => bail!(Error::BadQueryResultType),
            }
        }
        None => Ok(None),
    }
}

/// Return the last time the Sync 1.5 password with the given `uuid` was modified, either locally or
/// remotely; or `None`, if, if no such password is known.
fn time_sync_password_modified<Q>(queryable: &Q, uuid: SyncGuid) -> Result<Option<DateTime<Utc>>>
where Q: Queryable
{
    let remote_time_sync_password_modified = {
        let q = r#"[:find
                ?t .
                :in
                ?uuid
                :where
                [?sp :sync.password/uuid ?uuid]
                [?sp :sync.password/serverModified ?t]
               ]"#;

        let result = queryable.q_once(q, QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid))]))?.into_scalar()?;

        match result {
            Some(Binding::Scalar(TypedValue::Instant(t))) => t,
            Some(_) => bail!(Error::BadQueryResultType),
            None => return Ok(None),
        }
    };

    info!("time_sync_password_modified: remote_time_sync_password_modified: {:?}", remote_time_sync_password_modified);

    let local_time_sync_password_modified = {
        let q = r#"[:find
                 (max ?txI) .
                :in
                ?uuid
                :where
                [?sp :sync.password/uuid ?uuid]
                [?sp :sync.password/materialTx ?materialTx]

                (or-join [?sp ?a ?tx]
                 (and
                  [?sp :sync.password/credential ?c]
                  [?c ?a _ ?tx]
                  [(ground [:credential/id :credential/username :credential/password]) [?a ...]])
                 (and
                  [?f :form/syncPassword ?sp]
                  [?f ?a _ ?tx]
                  [(ground [:form/hostname :form/usernameField :form/passwordField :form/submitUrl :form/httpRealm]) [?a ...]]))

               [(tx-after ?tx ?materialTx)]
               [?tx :db/txInstant ?txI]

               ]"#;

        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid))]);

        let result = queryable.q_once(q, inputs)?.into_scalar()?;

        match result {
            Some(Binding::Scalar(TypedValue::Instant(t))) => Some(t),
            Some(_) => bail!(Error::BadQueryResultType),
            None => None,
        }
    };

    info!("time_sync_password_modified: local_time_sync_password_modified: {:?}", local_time_sync_password_modified);

    Ok(Some(match local_time_sync_password_modified {
        Some(t) if t > remote_time_sync_password_modified => t,
        _ => remote_time_sync_password_modified,
    }))
}

/// Return the number of time the Sync 1.5 password with given `uuid` was used, locally or remotely.
///
/// This adjoins recent local usage events onto the materialized remote usage timestamp.
fn times_used<Q>(queryable: &Q, uuid: SyncGuid) -> Result<u64>
where Q: Queryable
{
    let sync_mirror = {
        let q = r#"[:find
                [?timesUsed ?tx]
                :in
                ?uuid
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/timesUsed ?timesUsed]
                [?sl :sync.password/metadataTx ?tx]
               ]"#;

        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid))]);

        let sync_mirror: Result<_> = match queryable.q_once(q, inputs)?.into_tuple()? {
            Some((Binding::Scalar(TypedValue::Long(times_used)), Binding::Scalar(TypedValue::Ref(tx)))) => {
                let times_used = if times_used > 0 {
                    times_used as u64
                } else {
                    0
                };
                Ok(Some((times_used, tx)))
            },
            None => Ok(None),
            _ => bail!(Error::BadQueryResultType),
        };

        sync_mirror?
    };

    info!("times_used: sync_mirror: {:?}", sync_mirror);

    let (q, sync_tx) = if let Some((_, sync_tx)) = sync_mirror {
        let q = r#"[:find
                (count ?l) .
                :in
                ?uuid ?sync_tx
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/credential ?c]
                [?l :login/credential ?c]
                [?l :login/at _ ?login-tx]
                [(tx-after ?login-tx ?sync_tx)]
               ]"#;
        (q, sync_tx)
    } else {
        let q = r#"[:find
                (count ?l) .
                :in
                ?uuid ?sync_tx
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/credential ?c]
                [?l :login/credential ?c]
                [?l :login/at _]
               ]"#;
        (q, 0)
    };

    let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid)),
                                                       (var!(?sync_tx), TypedValue::Ref(sync_tx))]);

    let local_times_used: Result<_> = match queryable.q_once(q, inputs)?.into_scalar()? {
        Some(Binding::Scalar(TypedValue::Long(times_used))) => {
            if times_used > 0 {
                Ok(times_used as u64)
            } else {
                Ok(0)
            }
        },
        None => Ok(0),
        _ => bail!(Error::BadQueryResultType),
    };
    let local_times_used = local_times_used?;

    info!("times_used: local_times_used: {:?}", local_times_used);

    let times_used = if let Some((remote_times_used, _)) = sync_mirror {
        remote_times_used + local_times_used
    } else {
        local_times_used
    };

    Ok(times_used)
}

/// Return the last time the Sync 1.5 password with given `uuid` was used, locally or remotely.
///
/// This adjoins recent local usage events onto the materialized remote usage timestamp.
fn time_last_used<Q>(queryable: &Q, uuid: SyncGuid) -> Result<DateTime<Utc>>
    where Q: Queryable
{
    // We only care about local usages after the last tx we uploaded.

    let sync_mirror = {
        // Scope borrow of store.
        let q = r#"[:find
                [?timeLastUsed ?tx]
                :in
                ?uuid
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/timeLastUsed ?timeLastUsed]
                [?sl :sync.password/metadataTx ?tx]
               ]"#;

        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(uuid.clone()))]);

        let sync_mirror: Result<_> = match queryable.q_once(q, inputs)?.into_tuple()? {
            Some((Binding::Scalar(TypedValue::Instant(time_last_used)), Binding::Scalar(TypedValue::Ref(tx)))) =>
                Ok(Some((time_last_used, tx))),
            None => Ok(None),
            _ => bail!(Error::BadQueryResultType),
        };

        sync_mirror?
    };

    info!("time_last_used: sync_mirror: {:?}", sync_mirror);

    let (q, sync_tx) = if let Some((_, sync_tx)) = sync_mirror {
        let q = r#"[:find
                (max ?at) .
                :in
                ?uuid ?sync_tx
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/credential ?c]
                [?l :login/credential ?c]
                [?l :login/at ?at ?login-tx]
                [(tx-after ?login-tx ?sync_tx)]
               ]"#;
        (q, sync_tx)
    } else {
        let q = r#"[:find
                (max ?at) .
                :in
                ?uuid ?sync_tx
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/credential ?c]
                [?l :login/credential ?c]
                [?l :login/at ?at]
               ]"#;
        (q, 0)
    };

    let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(uuid)),
                                                       (var!(?sync_tx), TypedValue::Ref(sync_tx))]);

    let local_time_last_used: Result<_> = match queryable.q_once(q, inputs)?.into_scalar()? {
        Some(Binding::Scalar(TypedValue::Instant(time_last_used))) => Ok(Some(time_last_used)),
        None => Ok(None),
            _ => bail!(Error::BadQueryResultType),
    };

    let local_time_last_used = local_time_last_used?.unwrap_or_else(|| Utc.timestamp(0, 0));

    let time_last_used = if let Some((remote_time_last_used, _)) = sync_mirror {
        remote_time_last_used.max(local_time_last_used)
    } else {
        local_time_last_used
    };

    Ok(time_last_used)
}

/// Return the last time the Sync 1.5 password with given `uuid` was changed, locally or remotely.
///
/// This adjoins recent local changes onto the materialized remote change timestamp.
fn time_password_changed<Q>(queryable: &Q, uuid: SyncGuid) -> Result<Option<DateTime<Utc>>>
    where Q: Queryable
{

    let remote_time_password_changed = {
        let q = r#"[:find
                ?timePasswordChanged .
                :in
                ?uuid
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/timePasswordChanged ?timePasswordChanged]
               ]"#;

        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid))]);

        let remote_time_password_changed = queryable.q_once(q, inputs)?.into_scalar()?;

        let remote_time_password_changed = match remote_time_password_changed {
            Some(Binding::Scalar(TypedValue::Instant(time_password_changed))) => time_password_changed,
            Some(_) => bail!(Error::BadQueryResultType),
            None => return Ok(None),
        };

        remote_time_password_changed
    };

    info!("time_last_used: remote_time_password_changed: {:?}", remote_time_password_changed);

    // This is basically credentials::last_modified, but keyed by Sync uuid rather than credential
    // id; and using `materialTx` to filter.
    let local_time_password_changed = {
        let q = r#"[:find
                [?materialTx ?username-tx ?username-txInstant ?password-tx ?password-txInstant]
                :in
                ?uuid
                :where
                [?sl :sync.password/uuid ?uuid]
                [?sl :sync.password/materialTx ?materialTx]
                [?sl :sync.password/credential ?credential]
                [?credential :credential/username ?username ?username-tx]
                [?username-tx :db/txInstant ?username-txInstant]
                [?credential :credential/password ?password ?password-tx]
                [?password-tx :db/txInstant ?password-txInstant]]"#;

        let inputs = QueryInputs::with_value_sequence(vec![(var!(?uuid), TypedValue::typed_string(&uuid))]);

        match queryable.q_once(q, inputs)?.into_tuple()? {
            Some((Binding::Scalar(TypedValue::Ref(material_tx)),
                  Binding::Scalar(TypedValue::Ref(username_tx)),
                  Binding::Scalar(TypedValue::Instant(username_tx_instant)),
                  Binding::Scalar(TypedValue::Ref(password_tx)),
                  Binding::Scalar(TypedValue::Instant(password_tx_instant)))) => {
                Some((material_tx,
                      username_tx,
                      username_tx_instant.clone(),
                      password_tx,
                      password_tx_instant.clone()))
            },
            None => None,
            _ => bail!(Error::BadQueryResultType),
        }
    };

    info!("time_last_used: local_time_password_changed: {:?}", local_time_password_changed);

    let mut is = vec![];
    is.push(remote_time_password_changed);

    match local_time_password_changed {
        Some((material_tx, utx, utxi, ptx, ptxi)) => {
            if utx > material_tx {
                is.push(utxi);
            }
            if ptx > material_tx {
                is.push(ptxi);
            }
        },
        None => (),
    }

    Ok(is.into_iter().max())
}

/// Add assertions corresponding to the given Sync 1.5 `password` to the given `builder`, assuming
/// that the underlying credential has the given `credential_id`.
///
/// N.b., this function isn't composable quite yet: it uses named tempids with fixed names where it
/// should use an equivalent of Lisp's gensym.
fn build_sync_password(
    builder: &mut TermBuilder,
    password: &ServerPassword,
    credential_id: CredentialId)
    -> Result<()> {

    let c = builder.named_tempid("c");
    builder.add(c.clone(),
                CREDENTIAL_ID.clone(),
                TypedValue::typed_string(credential_id))?;

    let sl = builder.named_tempid("sl");
    builder.add(sl.clone(),
                SYNC_PASSWORD_UUID.clone(),
                TypedValue::typed_string(&password.uuid))?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_CREDENTIAL.clone(),
                c)?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_SERVER_MODIFIED.clone(),
                TypedValue::Instant(password.modified.clone()))?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_TIMES_USED.clone(),
                TypedValue::Long(password.times_used as i64))?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_TIME_CREATED.clone(),
                TypedValue::Instant(password.time_created.clone()))?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_TIME_LAST_USED.clone(),
                TypedValue::Instant(password.time_last_used.clone()))?;
    builder.add(sl.clone(),
                SYNC_PASSWORD_TIME_PASSWORD_CHANGED.clone(),
                TypedValue::Instant(password.time_password_changed.clone()))?;

    let f = builder.named_tempid("f");
    builder.add(f.clone(),
                FORM_SYNC_PASSWORD.clone(),
                sl)?;
    builder.add(f.clone(),
                FORM_HOSTNAME.clone(),
                TypedValue::typed_string(&password.hostname))?;
    if let Some(ref username_field) = password.username_field {
        builder.add(f.clone(),
                    FORM_USERNAME_FIELD.clone(),
                    TypedValue::typed_string(&username_field))?;
    }
    if let Some(ref password_field) = password.password_field {
        builder.add(f.clone(),
                    FORM_PASSWORD_FIELD.clone(),
                    TypedValue::typed_string(&password_field))?;
    }

    match password.target {
        FormTarget::FormSubmitURL(ref form_submit_url) => {
            builder.add(f.clone(),
                        FORM_SUBMIT_URL.clone(),
                        TypedValue::typed_string(form_submit_url))?;
        },
        FormTarget::HttpRealm(ref http_realm) => {
            builder.add(f.clone(),
                        FORM_HTTP_REALM.clone(),
                        TypedValue::typed_string(http_realm))?;
        },
    }

    Ok(())
}

/// Merge the remote `username` and `password` into the credential with `id`, using the given
/// `remote_modified` time to compare to the local modified time of the credential.
///
/// It is an error if a credential with the given `id` does not exist.
fn merge_into_credential(
    in_progress: &InProgress,
    builder: &mut TermBuilder,
    id: CredentialId,
    remote_modified: DateTime<Utc>,
    username: Option<String>,
    password: String)
    -> Result<()> {

    let local_modified = match credentials::time_last_modified(in_progress, id.clone())? {
        Some(x) => x,
        None => bail!(Error::BadQueryResultType),
    };

    debug!("merge_into_credential({}): remote modified {}, local modified {}",
           &id.0, remote_modified, local_modified);

    let c = builder.named_tempid("c");
    builder.add(c.clone(),
                CREDENTIAL_ID.clone(),
                TypedValue::typed_string(id.clone()))?;

    // We either accept all the remote material changes or we keep some local material changes.
    //
    // If we accept all the remote changes there are no local material changes to upload, and we
    // advance materialTx, which means this login won't be considered materially changed when we
    // check for logins to upload.
    //
    // If we keep at least one local material change, then we need to upload the merged login.  We
    // don't advance materialTx at all, which means this login will be considered materially changed
    // when we check for logins to upload.
    let remote_later = remote_modified > local_modified;
    if remote_later  {
        // TODO: handle optional username.
        info!("merge_into_credential({}): remote modified later than both local username and password; setting username {}, password {}",
              &id.0, username.clone().unwrap(), password);

        // TODO: work through what happens if only one side has a username.
        builder.add(c.clone(),
                    CREDENTIAL_USERNAME.clone(),
                    TypedValue::String(username.unwrap().into()))?; // XXX.

        builder.add(c.clone(),
                    CREDENTIAL_PASSWORD.clone(),
                    TypedValue::String(password.into()))?;

        let sl = builder.named_tempid("sl");
        builder.add(sl.clone(),
                    SYNC_PASSWORD_CREDENTIAL.clone(),
                    c.clone())?;
        builder.add(sl.clone(),
                    SYNC_PASSWORD_MATERIAL_TX.clone(),
                    TermBuilder::tx_function("transaction-tx"))?;
    } else {
        info!("merge_into_credential({}): local modified later than either remote username or password; keeping (and uploading) local modifications",
              &id.0);
    }

    Ok(())
}

/// Apply the given (changed) `password` against the local store.
pub fn apply_password(in_progress: &mut InProgress, password: ServerPassword) -> Result<TxReport> {
    enum Either<A, B> {
        Left(A),
        Right(B),
    }

    let id = match find_credential_id_by_sync_password_uuid(in_progress, password.uuid.clone())? {
        Some(id) => Some(Either::Left(id)),
        None => {
            // TODO: handle optional usernames.
            find_credential_by_content(in_progress,
                                       password.username.clone().unwrap(),
                                       password.password.clone())?
                .map(|c| Either::Right(c.id))
        },
    };

    let mut builder = TermBuilder::new();

    match id {
        None => {
            info!("apply_password: no existing credential for sync uuid {:?}", password.uuid);

            // Nothing found locally.  Add the credential and the sync password directly to the store, and
            // commit the sync tx at the same time.
            let id = CredentialId(password.uuid.0.clone());

            let credential = Credential {
                id: id.clone(),
                username: password.username.clone(),
                password: password.password.clone(),
                created_at: password.time_created.clone(),
                title: None,
            };

            build_credential(&mut builder, credential)?;
            build_sync_password(&mut builder, &password, id.clone())?;

            // Set metadataTx and materialTx to :db/tx.
            let c = builder.named_tempid("c"); // This is fun!  We could have collision of tempids across uses.
            builder.add(c.clone(),
                        CREDENTIAL_ID.clone(),
                        TypedValue::typed_string(id))?;
            let sl = builder.named_tempid("sl");
            builder.add(sl.clone(),
                        SYNC_PASSWORD_CREDENTIAL.clone(),
                        TypedValue::typed_string("c"))?;
            builder.add(sl.clone(),
                        SYNC_PASSWORD_MATERIAL_TX.clone(),
                        TermBuilder::tx_function("transaction-tx"))?;
            builder.add(sl.clone(),
                        SYNC_PASSWORD_METADATA_TX.clone(),
                        TermBuilder::tx_function("transaction-tx"))?;
        }

        Some(Either::Left(id)) => {
            info!("apply_password: existing credential {:?} associated with sync password for sync uuid {:?}", id, password.uuid);

            // We have an existing Sync password.  We need to merge the new changes into the credential
            // based on timestamps; we can't do better.
            build_sync_password(&mut builder, &password, id.clone())?;
            // Sets at most materialTx.
            merge_into_credential(&in_progress,
                                  &mut builder,
                                  id.clone(),
                                  password.modified,
                                  password.username.clone(),
                                  password.password.clone())?;
        }

        Some(Either::Right(id)) => {
            info!("apply_password: existing credential {:?} content matched for sync uuid {:?}", id, password.uuid);

            // We content matched.  We need to merge the new changes into the credential based on
            // timestamps; we can't do better.
            build_sync_password(&mut builder, &password, id.clone())?;
            // Sets at most materialTx.
            merge_into_credential(&in_progress,
                                  &mut builder,
                                  id.clone(),
                                  password.modified,
                                  password.username.clone(),
                                  password.password.clone())?;
        }
    }

    in_progress.transact_builder(builder).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use mentat::{
        FromMicros,
    };

    // TODO: either expose debug dumping for tests from Mentat or replace with standard queries.
    // use mentat::conn::{
    //     Dumpable,
    // };

    use super::*;

    use credentials::{
        add_credential,
        delete_by_id,
        touch_by_id,
    };

    use tests::{
        testing_store,
    };

    lazy_static! {
        static ref PASSWORD1: ServerPassword = {
            ServerPassword {
                modified: DateTime::<Utc>::from_micros(1523908142550),
                uuid: SyncGuid("{c5144948-fba1-594b-8148-ff70c85ee19a}".into()),
                hostname: "https://oauth-sync.dev.lcip.org".into(),
                target: FormTarget::FormSubmitURL("https://oauth-sync.dev.lcip.org/post".into()),
                username: Some("username@mockmyid.com".into()),
                password: "password".into(),
                username_field: Some("email".into()),
                password_field: None,
                time_created: DateTime::<Utc>::from_micros(1523908112453),
                time_password_changed: DateTime::<Utc>::from_micros(1523908112453),
                time_last_used: DateTime::<Utc>::from_micros(1000),
                times_used: 12,
            }
        };

        static ref PASSWORD2: ServerPassword = {
            ServerPassword {
                modified: DateTime::<Utc>::from_micros(1523909142550),
                uuid: SyncGuid("{d2c78792-1528-4026-afcb-6bd927f36a45}".into()),
                hostname: "https://totally-different.com".into(),
                target: FormTarget::FormSubmitURL("https://auth.totally-different.com".into()),
                username: Some("username@mockmyid.com".into()),
                password: "totally-different-password".into(),
                username_field: Some("auth_username".into()),
                password_field: Some("auth_password".into()),
                time_created: DateTime::<Utc>::from_micros(1523909141550),
                time_password_changed: DateTime::<Utc>::from_micros(1523909142550),
                time_last_used: DateTime::<Utc>::from_micros(1523909142550),
                times_used: 1,
            }
        };
    }

    #[test]
    fn test_get_sync_password() {
        // Verify that applying a password and then immediately reading it back yields the original
        // data.

        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, Some(PASSWORD1.clone()));

        let sp = get_sync_password(&in_progress,
                                   PASSWORD2.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, Some(PASSWORD2.clone()));

        let sp = get_sync_password(&in_progress,
                                   "nonexistent id".into()).expect("to get_sync_password");
        assert_eq!(sp, None);
    }

    #[test]
    fn test_get_sync_password_with_deleted_credential() {
        // Verify that applying a password, deleting its underlying credential, and then immediately
        // reading it back doesn't return the Sync 1.5 password.  This is one of many possible
        // choices for representing local deletion.

        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");

        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, Some(PASSWORD1.clone()));

        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        delete_by_id(&mut in_progress, PASSWORD1.uuid.0.clone().into()).expect("to delete_by_id");

        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, None);
    }

    #[test]
    fn test_get_all_sync_passwords() {
        // Verify that applying passwords and then immediately reading them all back yields the
        // original data.

        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply 1");
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply 2");

        let sps = get_all_sync_passwords(&in_progress).expect("to get_all_sync_passwords");
        assert_eq!(sps, vec![PASSWORD1.clone(), PASSWORD2.clone()]);
    }

    #[test]
    fn test_apply_twice() {
        // Verify that applying a password twice doesn't do anything the second time through.

        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");

        // TODO: either expose debug dumping for tests from Mentat or replace with standard queries.
        // let t = in_progress.dump_last_transaction().expect("transaction");
        // assert_eq!(t.into_vector().expect("vector").len(), 1); // Just the :db/txInstant.
    }

    #[test]
    fn test_remote_evolved() {
        // Verify that when there are no local changes, applying a remote record that has evolved
        // takes the remote changes.

        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        let mut password1 = PASSWORD1.clone();

        apply_password(&mut in_progress, password1.clone()).expect("to apply");

        password1.modified = ::mentat::now();
        password1.password = "password2".into();
        password1.password_field = Some("password".into());
        password1.times_used = 13;

        apply_password(&mut in_progress, password1.clone()).expect("to apply");

        let sp = get_sync_password(&in_progress,
                                   password1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, Some(password1.clone()));
    }

    #[test]
    fn test_get_modified_sync_passwords_to_upload() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        let report2 = apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        // But if there are no local changes, we shouldn't propose any records to re-upload.
        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);

        // Now, let's modify locally an existing credential connected to a Sync 1.5 record.
        //
        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        let mut builder = TermBuilder::new();
        builder.add(TermBuilder::lookup_ref(CREDENTIAL_ID.clone(), TypedValue::String(PASSWORD1.uuid.0.clone().into())),
                    CREDENTIAL_USERNAME.clone(),
                    TypedValue::typed_string("us3rnam3@mockymid.com")).expect("add");
        builder.add(TermBuilder::lookup_ref(CREDENTIAL_ID.clone(), TypedValue::String(PASSWORD1.uuid.0.clone().into())),
                    CREDENTIAL_PASSWORD.clone(),
                    TypedValue::typed_string("pa33w3rd")).expect("add");
        let report1 = in_progress.transact_builder(builder).expect("to transact");

        // TODO: either expose debug dumping for tests from Mentat or replace with standard queries.
        // // Just for our peace of mind.  One add and one retract per
        // // {username,password}, and the :db/txInstant.
        // let t = in_progress.dump_last_transaction().expect("transaction");
        // assert_eq!(t.into_vector().expect("vector").len(), 5);

        // Our local change results in a record needing to be uploaded remotely.
        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");

        let mut password1 = PASSWORD1.clone();
        password1.username = Some("us3rnam3@mockymid.com".into());
        password1.password = "pa33w3rd".into();
        password1.modified = report1.tx_instant;
        password1.time_password_changed = report1.tx_instant;
        assert_eq!(sp, vec![password1.clone()]);

        // Suppose we disconnect, so that the last materialTx is TX0 (and the last metadataTx is
        // also TX0), and then reconnect.  We'll have Sync 1.5 data in the store, and we'll need
        // to upload it all.
        reset_client(&mut in_progress).expect("to reset_client");

        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");

        // The credential in the store postdates the materialTx (== TX0), so we'll re-upload
        // these as records with timestamps that aren't identical to upstream.  I think it would
        // be better to re-populate the server with records identical to the earlier records,
        // but I don't think it's necessary to do so, so for now I'm avoiding that hassle.
        let mut password2 = PASSWORD2.clone();
        password2.modified = report2.tx_instant;
        password2.time_password_changed = report2.tx_instant;
        assert_eq!(sp.len(), 2);
        assert_eq!(sp[0], password1);
        assert_eq!(sp[1], password2);
        assert_eq!(sp, vec![password1.clone(), password2.clone()]);
    }

    #[test]
    fn test_get_deleted_sync_password_uuids_to_upload() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        // Deletion is a global operation in our Sync 1.5 data model, meaning that we don't take
        // into account the current Sync tx when determining if something has been deleted:
        // absence is all that matters.
        let sp = get_deleted_sync_password_uuids_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);

        // Now, let's delete an existing credential connected to a Sync 1.5 record.  Right now
        // is when we want to be able to :db/retractEntity a lookup-ref; see
        // https://github.com/mozilla/mentat/issues/378.
        //
        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        let mut builder = TermBuilder::new();
        builder.retract(TermBuilder::lookup_ref(CREDENTIAL_ID.clone(), TypedValue::String(PASSWORD1.uuid.0.clone().into())),
                        CREDENTIAL_ID.clone(),
                        TypedValue::String(PASSWORD1.uuid.0.clone().into())).expect("add");
        in_progress.transact_builder(builder).expect("to transact");

        // TODO: either expose debug dumping for tests from Mentat or replace with standard queries.
        // // Just for our peace of mind.
        // let t = in_progress.dump_last_transaction().expect("transaction");
        // assert_eq!(t.into_vector().expect("vector").len(), 2); // One retract, and the :db/txInstant.

        // The record's gone, Jim!
        let sp = get_deleted_sync_password_uuids_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![PASSWORD1.uuid.clone()]);

        // We can also sever the link between the Sync 1.5 record and the underlying credential.
        let mut builder = TermBuilder::new();
        builder.retract(TermBuilder::lookup_ref(SYNC_PASSWORD_UUID.clone(), TypedValue::String(PASSWORD2.uuid.0.clone().into())),
                        SYNC_PASSWORD_CREDENTIAL.clone(),
                        TermBuilder::lookup_ref(CREDENTIAL_ID.clone(), TypedValue::String(PASSWORD2.uuid.0.clone().into()))).expect("add");
        in_progress.transact_builder(builder).expect("to transact");

        // TODO: either expose debug dumping for tests from Mentat or replace with standard queries.
        // let t = in_progress.dump_last_transaction().expect("transaction");
        // assert_eq!(t.into_vector().expect("vector").len(), 2); // One retract, and the :db/txInstant.

        // Now both records are gone.
        let sp = get_deleted_sync_password_uuids_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![PASSWORD1.uuid.clone(), PASSWORD2.uuid.clone()]);
    }

    #[test]
    fn test_mark_synced_by_sync_uuids() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        let report1 = apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        let report2 = apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);

        // Suppose we disconnect, so that the last sync tx is TX0, and then reconnect.  We'll
        // have Sync 1.5 data in the store, and we'll need to upload it all.
        reset_client(&mut in_progress).expect("to reset_client");

        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");

        let mut password1 = PASSWORD1.clone();
        password1.modified = report1.tx_instant;
        password1.time_password_changed = report1.tx_instant;
        let mut password2 = PASSWORD2.clone();
        password2.modified = report2.tx_instant;
        password2.time_password_changed = report2.tx_instant;
        assert_eq!(sp, vec![password1.clone(), password2.clone()]);

        // Mark one password synced, and the other one will need to be uploaded.
        let synced_tx_id = in_progress.last_tx_id();
        let uuids = vec![PASSWORD1.uuid.clone()];
        mark_synced_by_sync_uuids(&mut in_progress, uuids.clone(), synced_tx_id).expect("to mark synced by sync uuids");

        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![password2.clone()]);

        // Mark everything unsynced, and then everything synced, and we won't upload anything.
        reset_client(&mut in_progress).expect("to reset_client");

        let synced_tx_id = in_progress.last_tx_id();
        let uuids = vec![PASSWORD1.uuid.clone(), PASSWORD2.uuid.clone()];
        mark_synced_by_sync_uuids(&mut in_progress, uuids.clone(), synced_tx_id).expect("to mark synced by sync uuids");

        let sp = get_modified_sync_passwords_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);
    }

    #[test]
    fn test_delete_by_sync_uuid() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        delete_by_sync_uuid(&mut in_progress, PASSWORD1.uuid.clone()).expect("to delete by sync uuid");

        // The record's gone.
        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, None);

        // And moreover, we won't try to upload a tombstone.
        let sp = get_deleted_sync_password_uuids_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);

        // If we try to delete again, that's okay.
        delete_by_sync_uuid(&mut in_progress, PASSWORD1.uuid.clone()).expect("to delete by sync uuid");


        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, None);

        // The other password wasn't deleted.
        let sp = get_sync_password(&in_progress,
                                   PASSWORD2.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, Some(PASSWORD2.clone()));
    }

    #[test]
    fn test_delete_by_sync_uuids() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");

        let uuids = vec![PASSWORD1.uuid.clone(), PASSWORD2.uuid.clone()];
        delete_by_sync_uuids(&mut in_progress, uuids.clone()).expect("to delete by sync uuids");

        // The record's gone.
        let sp = get_sync_password(&in_progress,
                                   PASSWORD1.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, None);

        let sp = get_sync_password(&in_progress,
                                   PASSWORD2.uuid.clone()).expect("to get_sync_password");
        assert_eq!(sp, None);

        // And moreover, we won't try to upload a tombstone.
        let sp = get_deleted_sync_password_uuids_to_upload(&in_progress).expect("to get_sync_password");
        assert_eq!(sp, vec![]);

        // If we try to delete again, that's okay.
        delete_by_sync_uuids(&mut in_progress, uuids.clone()).expect("to delete by sync uuid");
    }

    #[test]
    fn test_get_new_credential_ids() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // An existing Sync 1.5 password doesn't get flagged as new.
        apply_password(&mut in_progress, PASSWORD1.clone()).expect("to apply");
        assert_eq!(get_new_credential_ids(&in_progress).expect("to fetch get_new_credentials_ids"), vec![]);

        // If we first add a credential, it gets flagged.
        //
        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        let id = CredentialId(PASSWORD2.uuid.0.clone());
        let credential = Credential {
            id: id.clone(),
            username: PASSWORD2.username.clone(),
            password: PASSWORD2.password.clone(),
            created_at: PASSWORD2.time_created.clone(),
            title: None,
        };
        add_credential(&mut in_progress, credential).expect("add_credential");
        assert_eq!(get_new_credential_ids(&in_progress).expect("to fetch get_new_credentials_ids"), vec![id.clone()]);

        // If we then fill the rest of the Sync 1.5 password, we're no longer flagged.
        apply_password(&mut in_progress, PASSWORD2.clone()).expect("to apply");
        assert_eq!(get_new_credential_ids(&in_progress).expect("to fetch get_new_credentials_ids"), vec![]);
    }

    #[test]
    fn test_times_used() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // We get just the remote count initially.
        apply_password(&mut in_progress, PASSWORD1.clone()).unwrap();
        assert_eq!(12, times_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());

        // Couple of local usages.
        //
        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        touch_by_id(&mut in_progress, PASSWORD1.uuid.0.clone().into(), None).unwrap();
        let synced_tx_id = in_progress.last_tx_id();
        touch_by_id(&mut in_progress, PASSWORD1.uuid.0.clone().into(), None).unwrap();

        // We get the remote count plus the local count.
        assert_eq!(12 + 2, times_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());

        // If we now mark the record synced, the system assumes that we've uploaded the remote count
        // plus the local count _up to the synced transaction_.  Since we haven't updated the Sync
        // 1.5 password record metadata in the store, we get the old remote count (12) plus the
        // local count after the last synced metadataTx (1).
        //
        // TODO: update the remote count in the store after uploading metadata, so that we get the
        // correct remote count (13) plus the local count after the last synced metadataTx (1).
        mark_synced_by_sync_uuids(&mut in_progress, vec![PASSWORD1.uuid.clone()], synced_tx_id).unwrap();
        assert_eq!(12 + 1, times_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());
    }

    #[test]
    fn test_time_last_used() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // We get just the remote timestamp initially.
        apply_password(&mut in_progress, PASSWORD1.clone()).unwrap();
        assert_eq!(PASSWORD1.time_last_used.clone(), time_last_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());

        // Couple of local usages.
        //
        // Here we're using that the credential uuid and the Sync 1.5 uuid are the same; that's
        // not a stable assumption.
        touch_by_id(&mut in_progress, PASSWORD1.uuid.0.clone().into(), Some(DateTime::<Utc>::from_micros(3000))).unwrap();
        let synced_tx_id = in_progress.last_tx_id();
        touch_by_id(&mut in_progress, PASSWORD1.uuid.0.clone().into(), Some(DateTime::<Utc>::from_micros(2000))).unwrap();

        // We get the latest remote or local timestamp.
        assert_eq!(DateTime::<Utc>::from_micros(3000), time_last_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());

        // If we now mark the record synced, the system assumes that we've uploaded the remote count
        // plus the local count _up to the synced transaction_.  Since we haven't updated the Sync
        // 1.5 password record metadata in the database, we get the max of the old remote timestamp
        // (1000) and the latest local count after the last synced metadataTx (2000).
        //
        // TODO: update the remote timestamp in the store after uploading metadata, so that we get
        // the max of the remote timestamp (3000) and the local timestamp after the last
        // synced metadataTx (2000).
        mark_synced_by_sync_uuids(&mut in_progress, vec![PASSWORD1.uuid.clone()], synced_tx_id).unwrap();
        assert_eq!(DateTime::<Utc>::from_micros(2000), time_last_used(&in_progress, PASSWORD1.uuid.clone()).unwrap());
    }
}
