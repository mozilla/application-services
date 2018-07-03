// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

use mentat::{
    Binding,
    DateTime,
    Entid,
    QueryInputs,
    QueryResults,
    Queryable,
    StructuredMap,
    TxReport,
    TypedValue,
    Utc,
};

use mentat::entity_builder::{
    BuildTerms,
    TermBuilder,
};

use mentat::conn::{
    InProgress,
};

use errors::{
    Error,
    Result,
};
use types::{
    Credential,
    CredentialId,
};
use vocab::{
    CREDENTIAL_ID,
    CREDENTIAL_USERNAME,
    CREDENTIAL_PASSWORD,
    CREDENTIAL_CREATED_AT,
    CREDENTIAL_TITLE,
    LOGIN_AT,
    // TODO: connect logins to specific LOGIN_DEVICE.
    LOGIN_CREDENTIAL,
    // TODO: connect logins to LOGIN_FORM.
};

impl Credential {
    /// Produce a `Credential` from a structured map (as returned by a pull expression).
    pub(crate) fn from_structured_map(map: &StructuredMap) -> Option<Self> {
        let id = map[&*CREDENTIAL_ID].as_string().map(|x| (**x).clone()).map(CredentialId).unwrap(); // XXX
        let username = map.get(&*CREDENTIAL_USERNAME).and_then(|username| username.as_string()).map(|x| (**x).clone()); // XXX
        let password = map[&*CREDENTIAL_PASSWORD].as_string().map(|x| (**x).clone()).unwrap(); // XXX
        let created_at = map[&*CREDENTIAL_CREATED_AT].as_instant().map(|x| (*x).clone()).unwrap(); // XXX
        let title = map.get(&*CREDENTIAL_TITLE).and_then(|username| username.as_string()).map(|x| (**x).clone()); // XXX
        // TODO: device.

        Some(Credential {
            id,
            created_at,
            username,
            password,
            title,
        })
    }
}

/// Assert the given `credential` against the given `builder`.
///
/// N.b., this uses the (globally) named tempid "c", so it can't be used twice against the same
/// builder!
pub(crate) fn build_credential(builder: &mut TermBuilder, credential: Credential) -> Result<()> {
    let c = builder.named_tempid("c");

    builder.add(c.clone(),
                CREDENTIAL_ID.clone(),
                TypedValue::typed_string(credential.id))?;
    if let Some(username) = credential.username {
        builder.add(c.clone(),
                    CREDENTIAL_USERNAME.clone(),
                    TypedValue::String(username.into()))?;
    }
    builder.add(c.clone(),
                CREDENTIAL_PASSWORD.clone(),
                TypedValue::String(credential.password.into()))?;
    // TODO: set created to the transaction timestamp.  This might require implementing
    // (transaction-instant), which requires some thought because it is a "delayed binding".
    builder.add(c.clone(),
                CREDENTIAL_CREATED_AT.clone(),
                TypedValue::Instant(credential.created_at))?;
    if let Some(title) = credential.title {
        builder.add(c.clone(),
                    CREDENTIAL_TITLE.clone(),
                    TypedValue::String(title.into()))?;
    }

    Ok(())
}

/// Transact the given `credential` against the given `InProgress` write.
///
/// If a credential with the given ID exists, it will be modified in place.
pub fn add_credential(in_progress: &mut InProgress, credential: Credential) -> Result<TxReport> {
    let mut builder = TermBuilder::new();
    build_credential(&mut builder, credential.clone())?;
    in_progress.transact_builder(builder).map_err(|e| e.into())
}

/// Fetch the credential with given `id`.
pub fn get_credential<Q>(queryable: &Q, id: CredentialId) -> Result<Option<Credential>> where Q: Queryable {
    let q = r#"[:find
                (pull ?c [:credential/id :credential/username :credential/password :credential/createdAt :credential/title]) .
                :in
                ?id
                :where
                [?c :credential/id ?id]
               ]"#;

    let inputs = QueryInputs::with_value_sequence(vec![
        (var!(?id), TypedValue::typed_string(&id)),
    ]);

    let scalar = queryable.q_once(q, inputs)?.into_scalar()?;
    let credential = match scalar {
        Some(Binding::Map(cm)) => Ok(Credential::from_structured_map(cm.as_ref())),
        Some(_) => bail!(Error::BadQueryResultType),
        None => Ok(None),
    };

    credential
}

/// Fetch all known credentials.
///
/// No ordering is implied.
pub fn get_all_credentials<Q>(queryable: &Q) -> Result<Vec<Credential>>
where Q: Queryable {
    let q = r#"[
:find
 [?id ...]
:where
 [_ :credential/id ?id]
:order
 (asc ?id) ; We order for testing convenience.
]"#;

    let ids: Result<Vec<_>> = queryable.q_once(q, None)?
        .into_coll()?
        .into_iter()
        .map(|id| {
            match id {
                Binding::Scalar(TypedValue::String(id)) => Ok(CredentialId((*id).clone())),
                _ => bail!(Error::BadQueryResultType),
            }
        })
        .collect();
    let ids = ids?;

    // TODO: do this more efficiently.
    let mut cs = Vec::with_capacity(ids.len());

    for id in ids {
        get_credential(queryable, id)?.map(|c| cs.push(c));
    }

    Ok(cs)
}

/// Record a local usage of the credential with given `id`, optionally `at` the given timestamp.
pub fn touch_by_id(in_progress: &mut InProgress, id: CredentialId, at: Option<DateTime<Utc>>) -> Result<TxReport> {
    // TODO: Also record device.

    let mut builder = TermBuilder::new();
    let l = builder.named_tempid("l");

    // New login.
    builder.add(l.clone(),
                LOGIN_AT.clone(),
                // TODO: implement and use (tx-instant).
                TypedValue::Instant(at.unwrap_or_else(|| ::mentat::now())))?;
    builder.add(l.clone(),
                LOGIN_CREDENTIAL.clone(),
                TermBuilder::lookup_ref(CREDENTIAL_ID.clone(), TypedValue::typed_string(id)))?;

    in_progress.transact_builder(builder).map_err(|e| e.into())
}

/// Delete the credential with the given `id`, if one exists.
pub fn delete_by_id(in_progress: &mut InProgress, id: CredentialId) -> Result<()> {
    delete_by_ids(in_progress, ::std::iter::once(id))
}

/// Delete credentials with the given `ids`, if any exist.
pub fn delete_by_ids<I>(in_progress: &mut InProgress, ids: I) -> Result<()>
where I: IntoIterator<Item=CredentialId> {
    // TODO: implement and use some version of `:db/retractEntity`, rather than onerously deleting
    // credential data and usage data.
    //
    // N.b., I'm not deleting the dangling link from `:sync.password/credential` here.  That's a
    // choice; not deleting that link allows the Sync password to discover that its underlying
    // credential has been removed (although, deleting that link reveals the information as well).
    // Using `:db/retractEntity` in some form impacts this decision.
    let q = r#"[
:find
 ?e ?a ?v
:in
 ?id
:where
 (or-join [?e ?a ?v ?id]
  (and
   [?e :credential/id ?id]
   [?e ?a ?v])
  (and
   [?c :credential/id ?id]
   [?e :login/credential ?c]
   [?e ?a ?v]))
]"#;

    let mut builder = TermBuilder::new();

    for id in ids {
        let inputs = QueryInputs::with_value_sequence(vec![(var!(?id), TypedValue::typed_string(id))]);
        let results = in_progress.q_once(q, inputs)?.results;

        match results {
            QueryResults::Rel(vals) => {
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

/// Find a credential matching the given `username` and `password`, if one exists.
///
/// It is possible that multiple credentials match, in which case one is chosen at random.  (This is
/// an impedance mismatch between the model of logins we're driving towards and the requirements of
/// Sync 1.5 passwords to do content-aware merging.)
pub fn find_credential_by_content<Q>(queryable: &Q, username: String, password: String) -> Result<Option<Credential>>
where Q: Queryable
{
    let q = r#"[:find ?id .
                :in
                ?username ?password
                :where
                [?c :credential/id ?id]
                [?c :credential/username ?username]
                [?c :credential/password ?password]]"#;

    let inputs = QueryInputs::with_value_sequence(vec![(var!(?username), TypedValue::String(username.clone().into())),
                                                       (var!(?password), TypedValue::String(password.clone().into()))]);
    let id = match queryable.q_once(q, inputs)?.into_scalar()? {
        Some(x) => {
            match x.into_string() {
                Some(x) => CredentialId((*x).clone()),
                None => bail!(Error::BadQueryResultType),
            }
        }
        None => return Ok(None),
    };

    get_credential(queryable, id)
}

/// Return the number of times the credential with given `id` has been used locally, or `None` if
/// such a credential doesn't exist, optionally limiting to usages strictly after the given
/// `after_tx`.
// TODO: u64.
// TODO: filter by devices.
pub fn times_used<Q>(queryable: &Q, id: CredentialId, after_tx: Option<Entid>) -> Result<Option<i64>>
where Q: Queryable
{
    // TODO: Don't run this first query to determine if a credential (ID) exists.  This is only here
    // because it's surprisingly awkward to return `None` rather than `0` for a non-existent
    // credential ID.
    if get_credential(queryable, id.clone())?.is_none() {
        return Ok(None);
    }

    let q = r#"[:find
                (count ?l) .
                :in
                ?id ?after_tx
                :where
                [?c :credential/id ?id]
                [?l :login/credential ?c]
                [?l :login/at _ ?login-tx]
                [(tx-after ?login-tx ?after_tx)]]"#;

    // TODO: drop the comparison when `after_tx` is `None`.
    let values =
        QueryInputs::with_value_sequence(vec![(var!(?id), TypedValue::typed_string(&id)),
                                              (var!(?after_tx), TypedValue::Ref(after_tx.unwrap_or(0)))]);

    let local_times_used = match queryable.q_once(q, values)?.into_scalar()? {
        Some(Binding::Scalar(TypedValue::Long(times_used))) => Some(times_used), // TODO: work out overflow for u64.
        None => None,
        _ => bail!(Error::BadQueryResultType),
    };

    Ok(local_times_used)
}

/// Return the last time the credential with given `id` was used locally, or `None` if such a
/// credential doesn't exist, optionally limiting to usages strictly after the given `after_tx`.
// TODO: filter by devices.
pub fn time_last_used<Q>(queryable: &Q, id: CredentialId, after_tx: Option<Entid>) -> Result<Option<DateTime<Utc>>>
where Q: Queryable
{
    let q = r#"[:find
                (max ?at) .
                :in
                ?id ?after_tx
                :where
                [?c :credential/id ?id]
                [?l :login/credential ?c]
                [?l :login/at ?at ?login-tx]
                [(tx-after ?login-tx ?after_tx)]
               ]"#;

    // TODO: drop the comparison when `after_tx` is `None`.
    let values =
        QueryInputs::with_value_sequence(vec![(var!(?id), TypedValue::typed_string(id)),
                                              (var!(?after_tx), TypedValue::Ref(after_tx.unwrap_or(0)))]);

    let local_time_last_used = match queryable.q_once(q, values)?.into_scalar()? {
        Some(Binding::Scalar(TypedValue::Instant(time_last_used))) => Some(time_last_used),
        None => None,
        _ => bail!(Error::BadQueryResultType),
    };

    Ok(local_time_last_used)
}

/// Return the last time the credential with given `id` was modified locally, or `None` if such a
/// credential doesn't exist.
pub fn time_last_modified<Q>(queryable: &Q, id: CredentialId) -> Result<Option<DateTime<Utc>>>
where Q: Queryable
{
    // TODO: handle optional usernames.
    let q = r#"[:find
                [?username-txInstant ?password-txInstant]
                :in
                ?id
                :where
                [?credential :credential/id ?id]
                [?credential :credential/username ?username ?username-tx]
                [?username-tx :db/txInstant ?username-txInstant]
                [?credential :credential/password ?password ?password-tx]
                [?password-tx :db/txInstant ?password-txInstant]]"#;
    let inputs = QueryInputs::with_value_sequence(vec![(var!(?id), TypedValue::typed_string(id))]);

    match queryable.q_once(q, inputs)?.into_tuple()? {
        Some((Binding::Scalar(TypedValue::Instant(username_tx_instant)),
              Binding::Scalar(TypedValue::Instant(password_tx_instant)))) => {
            let last_modified = ::std::cmp::max(username_tx_instant, password_tx_instant);
            Ok(Some(last_modified))
        },
        None => Ok(None),
        _ => bail!(Error::BadQueryResultType),
    }
}

#[cfg(test)]
mod tests {
    use mentat::{
        FromMicros,
    };

    use super::*;

    use tests::{
        testing_store,
    };

    lazy_static! {
        static ref CREDENTIAL1: Credential = {
            Credential {
                id: CredentialId("1".into()),
                username: Some("user1@mockymid.com".into()),
                password: "password1".into(),
                created_at: DateTime::<Utc>::from_micros(1523908112453),
                title: None,
            }
        };

        static ref CREDENTIAL2: Credential = {
            Credential {
                id: CredentialId("2".into()),
                username: Some("user2@mockymid.com".into()),
                password: "password2".into(),
                created_at: DateTime::<Utc>::from_micros(1523909000000),
                title: Some("marché".into()),  // Observe accented character.
            }
        };

        static ref CREDENTIAL_WITHOUT_USERNAME: Credential = {
            Credential {
                id: CredentialId("3".into()),
                username: None,
                password: "password3".into(),
                created_at: DateTime::<Utc>::from_micros(1523909111111),
                title: Some("credential without username".into()),
            }
        };
    }

    #[test]
    fn test_credentials() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // First, let's add a single credential.
        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");

        let c = get_credential(&in_progress, CREDENTIAL1.id.clone()).expect("to get_credential 1");
        assert_eq!(Some(CREDENTIAL1.clone()), c);

        let cs = get_all_credentials(&in_progress).expect("to get_all_credentials 1");
        assert_eq!(vec![CREDENTIAL1.clone()], cs);

        // Now a second one.
        add_credential(&mut in_progress, CREDENTIAL2.clone()).expect("to add_credential 2");

        let c = get_credential(&in_progress, CREDENTIAL1.id.clone()).expect("to get_credential 1");
        assert_eq!(Some(CREDENTIAL1.clone()), c);

        let c = get_credential(&in_progress, CREDENTIAL2.id.clone()).expect("to get_credential 2");
        assert_eq!(Some(CREDENTIAL2.clone()), c);

        let cs = get_all_credentials(&in_progress).expect("to get_all_credentials 2");
        assert_eq!(vec![CREDENTIAL1.clone(), CREDENTIAL2.clone()], cs);
    }

    #[test]
    fn test_credential_without_username() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // Let's verify that we can serialize and deserialize a credential without a username.
        add_credential(&mut in_progress, CREDENTIAL_WITHOUT_USERNAME.clone()).unwrap();

        let c = get_credential(&in_progress, CREDENTIAL_WITHOUT_USERNAME.id.clone()).unwrap();
        assert_eq!(Some(CREDENTIAL_WITHOUT_USERNAME.clone()), c);

        let cs = get_all_credentials(&in_progress).unwrap();
        assert_eq!(vec![CREDENTIAL_WITHOUT_USERNAME.clone()], cs);
    }

    #[test]
    fn test_delete_by_id() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // First, let's add a few credentials.
        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");
        add_credential(&mut in_progress, CREDENTIAL2.clone()).expect("to add_credential 2");

        delete_by_id(&mut in_progress, CREDENTIAL1.id.clone()).expect("to delete by id");

        // The record's gone.
        let c = get_credential(&in_progress,
                               CREDENTIAL1.id.clone()).expect("to get_credential");
        assert_eq!(c, None);

        // If we try to delete again, that's okay.
        delete_by_id(&mut in_progress, CREDENTIAL1.id.clone()).expect("to delete by id when it's already deleted");

        let c = get_credential(&in_progress,
                               CREDENTIAL1.id.clone()).expect("to get_credential");
        assert_eq!(c, None);

        // The other password wasn't deleted.
        let c = get_credential(&in_progress,
                               CREDENTIAL2.id.clone()).expect("to get_credential");
        assert_eq!(c, Some(CREDENTIAL2.clone()));
    }

    #[test]
    fn test_delete_by_ids() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // First, let's add a few credentials.
        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");
        add_credential(&mut in_progress, CREDENTIAL2.clone()).expect("to add_credential 2");

        let iters = ::std::iter::once(CREDENTIAL1.id.clone()).chain(::std::iter::once(CREDENTIAL2.id.clone()));
        delete_by_ids(&mut in_progress, iters.clone()).expect("to delete_by_ids");

        // The records are gone.
        let c = get_credential(&in_progress,
                               CREDENTIAL1.id.clone()).expect("to get_credential");
        assert_eq!(c, None);

        let c = get_credential(&in_progress,
                               CREDENTIAL2.id.clone()).expect("to get_credential");
        assert_eq!(c, None);

        // If we try to delete again, that's okay.
        delete_by_ids(&mut in_progress, iters.clone()).expect("to delete_by_ids");
    }

    #[test]
    fn test_find_credential_by_content() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");

        let c = find_credential_by_content(&in_progress,
                                           CREDENTIAL1.username.clone().unwrap(),
                                           CREDENTIAL1.password.clone()).expect("to find_credential_by_content");
        assert_eq!(c, Some(CREDENTIAL1.clone()));

        let c = find_credential_by_content(&in_progress,
                                           "incorrect username".to_string(),
                                           CREDENTIAL1.password.clone()).expect("to find_credential_by_content");
        assert_eq!(c, None);

        let c = find_credential_by_content(&in_progress,
                                           CREDENTIAL1.username.clone().unwrap(),
                                           "incorrect password".to_string()).expect("to find_credential_by_content");
        assert_eq!(c, None);
    }

    #[test]
    fn test_times_used() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // First, let's add a few credentials.
        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");
        add_credential(&mut in_progress, CREDENTIAL2.clone()).expect("to add_credential 2");

        let report1 = touch_by_id(&mut in_progress, CREDENTIAL1.id.clone(), None).expect("touch_by_id");
        let now1 = ::mentat::now();

        let report2 = touch_by_id(&mut in_progress, CREDENTIAL2.id.clone(), None).expect("touch_by_id");
        let now2 = ::mentat::now();

        touch_by_id(&mut in_progress, CREDENTIAL1.id.clone(), Some(now1)).expect("touch_by_id");
        let report3 = touch_by_id(&mut in_progress, CREDENTIAL2.id.clone(), Some(now2)).expect("touch_by_id");

        assert_eq!(None, times_used(&in_progress, "unknown credential".into(), None).expect("times_used"));

        assert_eq!(Some(2), times_used(&in_progress, CREDENTIAL1.id.clone(), None).expect("times_used"));
        assert_eq!(Some(1), times_used(&in_progress, CREDENTIAL1.id.clone(), Some(report1.tx_id)).expect("times_used"));
        assert_eq!(Some(1), times_used(&in_progress, CREDENTIAL1.id.clone(), Some(report2.tx_id)).expect("times_used"));
        assert_eq!(Some(0), times_used(&in_progress, CREDENTIAL1.id.clone(), Some(report3.tx_id)).expect("times_used"));

        assert_eq!(Some(2), times_used(&in_progress, CREDENTIAL2.id.clone(), None).expect("times_used"));
        assert_eq!(Some(2), times_used(&in_progress, CREDENTIAL2.id.clone(), Some(report1.tx_id)).expect("times_used"));
        assert_eq!(Some(1), times_used(&in_progress, CREDENTIAL2.id.clone(), Some(report2.tx_id)).expect("times_used"));
        assert_eq!(Some(0), times_used(&in_progress, CREDENTIAL2.id.clone(), Some(report3.tx_id)).expect("times_used"));
    }

    #[test]
    fn test_last_time_used() {
        let mut store = testing_store();
        let mut in_progress = store.begin_transaction().expect("begun successfully");

        // First, let's add a few credentials.
        add_credential(&mut in_progress, CREDENTIAL1.clone()).expect("to add_credential 1");
        add_credential(&mut in_progress, CREDENTIAL2.clone()).expect("to add_credential 2");

        // Just so there is a visit for credential 2, in case there is an error across credentials.
        touch_by_id(&mut in_progress, CREDENTIAL2.id.clone(), None).expect("touch_by_id");

        touch_by_id(&mut in_progress, CREDENTIAL1.id.clone(), None).expect("touch_by_id");
        let now1 = ::mentat::now();
        touch_by_id(&mut in_progress, CREDENTIAL1.id.clone(), Some(now1)).expect("touch_by_id");

        assert_eq!(None, time_last_used(&in_progress, "unknown credential".into(), None).expect("time_last_used"));

        assert_eq!(Some(now1), time_last_used(&in_progress, CREDENTIAL1.id.clone(), None).expect("time_last_used"));

        // This is a little unusual.  We're going to record consecutive usages with timestamps going
        // backwards in time.
        let now2 = ::mentat::now();
        let report = touch_by_id(&mut in_progress, CREDENTIAL2.id.clone(), Some(now2)).expect("touch_by_id");
        touch_by_id(&mut in_progress, CREDENTIAL2.id.clone(), Some(now1)).expect("touch_by_id");

        assert_eq!(Some(now2), time_last_used(&in_progress, CREDENTIAL2.id.clone(), None).expect("time_last_used"));
        assert_eq!(Some(now1), time_last_used(&in_progress, CREDENTIAL2.id.clone(), Some(report.tx_id)).expect("time_last_used"));
    }
}
