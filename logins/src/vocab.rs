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
    InProgress,
    Keyword,
    ValueType,
};

use mentat::vocabulary;
use mentat::vocabulary::{
    VersionedStore,
};

use errors::{
    Result,
};

lazy_static! {
    // [:credential/username       :db.type/string  :db.cardinality/one]
    // [:credential/password       :db.type/string  :db.cardinality/one]
    // [:credential/created        :db.type/instant :db.cardinality/one]
    // An application might allow users to name their credentials; e.g., "My LDAP".
    // [:credential/title          :db.type/string  :db.cardinality/one]

    pub static ref CREDENTIAL_ID: Keyword = {
        kw!(:credential/id)
    };

    pub static ref CREDENTIAL_USERNAME: Keyword = {
        kw!(:credential/username)
    };

    pub static ref CREDENTIAL_PASSWORD: Keyword = {
        kw!(:credential/password)
    };

    pub static ref CREDENTIAL_CREATED_AT: Keyword = {
        kw!(:credential/createdAt)
    };

    pub static ref CREDENTIAL_TITLE: Keyword = {
        kw!(:credential/title)
    };

    pub static ref CREDENTIAL_VOCAB: vocabulary::Definition = {
        vocabulary::Definition {
            name: kw!(:org.mozilla/credential),
            version: 1,
            attributes: vec![
                (CREDENTIAL_ID.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .unique(vocabulary::attribute::Unique::Identity)
                 .multival(false)
                 .build()),
                (CREDENTIAL_USERNAME.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (CREDENTIAL_PASSWORD.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (CREDENTIAL_CREATED_AT.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
                (CREDENTIAL_TITLE.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
            ],
            pre: vocabulary::Definition::no_op,
            post: vocabulary::Definition::no_op,
        }
    };

    // This is metadata recording user behavior.
    // [:login/at                  :db.type/instant :db.cardinality/one]
    // [:login/device              :db.type/ref     :db.cardinality/one]
    // [:login/credential          :db.type/ref     :db.cardinality/one]
    // [:login/form                :db.type/ref     :db.cardinality/one]
    pub static ref LOGIN_AT: Keyword = {
        kw!(:login/at)
    };

    pub static ref LOGIN_DEVICE: Keyword = {
        kw!(:login/device)
    };

    pub static ref LOGIN_CREDENTIAL: Keyword = {
        kw!(:login/credential)
    };

    pub static ref LOGIN_FORM: Keyword = {
        kw!(:login/form)
    };

    pub static ref LOGIN_VOCAB: vocabulary::Definition = {
        vocabulary::Definition {
            name: kw!(:org.mozilla/login),
            version: 1,
            attributes: vec![
                (LOGIN_AT.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
                (LOGIN_DEVICE.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .build()),
                (LOGIN_CREDENTIAL.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .build()),
                (LOGIN_FORM.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .build()),
            ],
            pre: vocabulary::Definition::no_op,
            post: vocabulary::Definition::no_op,
        }
    };

    // A 'form' is either an HTTP login box _or_ a web form.
    // [:http/realm                :db.type/string  :db.cardinality/one]
    // It's possible that hostname or submitOrigin are unique-identity attributes.
    // [:form/hostname             :db.type/string  :db.cardinality/one]
    // [:form/submitOrigin         :db.type/string  :db.cardinality/one]
    // [:form/usernameField        :db.type/string  :db.cardinality/one]
    // [:form/passwordField        :db.type/string  :db.cardinality/one]
    // This is our many-to-many relation between forms and credentials.
    // [:form/credential           :db.type/ref     :db.cardinality/many]
    pub static ref FORM_HOSTNAME: Keyword = {
        kw!(:form/hostname)
    };

    pub static ref FORM_SUBMIT_URL: Keyword = {
        kw!(:form/submitUrl)
    };

    pub static ref FORM_USERNAME_FIELD: Keyword = {
        kw!(:form/usernameField)
    };

    pub static ref FORM_PASSWORD_FIELD: Keyword = {
        kw!(:form/passwordField)
    };

    pub static ref FORM_CREDENTIAL: Keyword = {
        kw!(:form/credential)
    };

    pub static ref FORM_HTTP_REALM: Keyword = {
        kw!(:form/httpRealm)
    };

    // This is arguably backwards.  In the future, we'd like forms to be independent of Sync 1.5
    // password records, in the way that we're making credentials independent of password records.
    // For now, however, we don't want to add an identifier and identify forms by content, so we're
    // linking a form to a unique Sync password.  Having the link go in this direction lets us
    // upsert the form.
    pub static ref FORM_SYNC_PASSWORD: Keyword = {
        kw!(:form/syncPassword)
    };

    pub static ref FORM_VOCAB: vocabulary::Definition = {
        vocabulary::Definition {
            name: kw!(:org.mozilla/form),
            version: 1,
            attributes: vec![
                (FORM_SYNC_PASSWORD.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .unique(vocabulary::attribute::Unique::Identity)
                 .build()),
                (FORM_HOSTNAME.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (FORM_SUBMIT_URL.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (FORM_USERNAME_FIELD.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (FORM_PASSWORD_FIELD.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
                (FORM_CREDENTIAL.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(true)
                 .build()),
                (FORM_HTTP_REALM.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .build()),
            ],
            pre: vocabulary::Definition::no_op,
            post: vocabulary::Definition::no_op,
        }
    };

    pub static ref SYNC_PASSWORD_CREDENTIAL: Keyword = {
        kw!(:sync.password/credential)
    };

    pub static ref SYNC_PASSWORD_UUID: Keyword = {
        kw!(:sync.password/uuid)
    };

    // Use materialTx for material change comparisons, metadataTx for metadata change
    // comparisons.  Downloading updates materialTx only.  We only use materialTx to
    // determine whether or not to upload.  Uploaded records are built using metadataTx,
    // however.  Successful upload sets both materialTx and metadataTx.
    pub static ref SYNC_PASSWORD_MATERIAL_TX: Keyword = {
        kw!(:sync.password/materialTx)
    };

    pub static ref SYNC_PASSWORD_METADATA_TX: Keyword = {
        kw!(:sync.password/metadataTx)
    };

    pub static ref SYNC_PASSWORD_SERVER_MODIFIED: Keyword = {
        kw!(:sync.password/serverModified)
    };

    pub static ref SYNC_PASSWORD_TIMES_USED: Keyword = {
        kw!(:sync.password/timesUsed)
    };

    pub static ref SYNC_PASSWORD_TIME_CREATED: Keyword = {
        kw!(:sync.password/timeCreated)
    };

    pub static ref SYNC_PASSWORD_TIME_LAST_USED: Keyword = {
        kw!(:sync.password/timeLastUsed)
    };

    pub static ref SYNC_PASSWORD_TIME_PASSWORD_CHANGED: Keyword = {
        kw!(:sync.password/timePasswordChanged)
    };

    pub static ref SYNC_PASSWORD_VOCAB: vocabulary::Definition = {
        vocabulary::Definition {
            name: kw!(:org.mozilla/sync.password),
            version: 1,
            attributes: vec![
                (SYNC_PASSWORD_CREDENTIAL.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .unique(vocabulary::attribute::Unique::Identity)
                 .build()),
                (SYNC_PASSWORD_UUID.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::String)
                 .multival(false)
                 .unique(vocabulary::attribute::Unique::Identity)
                 .build()),
                (SYNC_PASSWORD_MATERIAL_TX.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_METADATA_TX.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Ref)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_SERVER_MODIFIED.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_TIMES_USED.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Long)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_TIME_CREATED.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_TIME_LAST_USED.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
                (SYNC_PASSWORD_TIME_PASSWORD_CHANGED.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Instant)
                 .multival(false)
                 .build()),
            ],
            pre: vocabulary::Definition::no_op,
            post: vocabulary::Definition::no_op,
        }
    };

    pub static ref SYNC_PASSWORDS_LAST_SERVER_TIMESTAMP: Keyword = {
        kw!(:sync.passwords/lastServerTimestamp)
    };

    pub static ref SYNC_PASSWORDS_VOCAB: vocabulary::Definition = {
        vocabulary::Definition {
            name: kw!(:org.mozilla/sync.passwords),
            version: 1,
            attributes: vec![
                (SYNC_PASSWORDS_LAST_SERVER_TIMESTAMP.clone(),
                 vocabulary::AttributeBuilder::helpful()
                 .value_type(ValueType::Double)
                 .multival(false)
                 .build()),
            ],
            pre: vocabulary::Definition::no_op,
            post: vocabulary::Definition::no_op,
        }
    };
}

pub fn ensure_vocabulary(in_progress: &mut InProgress) -> Result<()> {
    debug!("Ensuring logins vocabulary is installed.");

    in_progress.verify_core_schema()?;

    in_progress.ensure_vocabulary(&CREDENTIAL_VOCAB)?;
    in_progress.ensure_vocabulary(&LOGIN_VOCAB)?;
    in_progress.ensure_vocabulary(&FORM_VOCAB)?;
    in_progress.ensure_vocabulary(&SYNC_PASSWORD_VOCAB)?;
    in_progress.ensure_vocabulary(&SYNC_PASSWORDS_VOCAB)?;

    Ok(())
}
