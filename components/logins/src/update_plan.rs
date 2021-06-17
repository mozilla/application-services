/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::login::{LocalLogin, Login, MirrorLogin, SyncStatus};
use crate::util;
use rusqlite::{named_params, Connection};
use sql_support::SqlInterruptScope;
use std::time::SystemTime;
use sync15::ServerTimestamp;
use sync_guid::Guid;

#[derive(Default, Debug, Clone)]
pub(crate) struct UpdatePlan {
    pub delete_mirror: Vec<Guid>,
    pub delete_local: Vec<Guid>,
    pub local_updates: Vec<MirrorLogin>,
    // the bool is the `is_overridden` flag, the i64 is ServerTimestamp in millis
    pub mirror_inserts: Vec<(Login, i64, bool)>,
    pub mirror_updates: Vec<(Login, i64)>,
}

impl UpdatePlan {
    pub fn plan_two_way_merge(&mut self, local: &Login, upstream: (Login, ServerTimestamp)) {
        let is_override = local.time_password_changed > upstream.0.time_password_changed;
        self.mirror_inserts
            .push((upstream.0, upstream.1.as_millis() as i64, is_override));
        if !is_override {
            self.delete_local.push(local.guid());
        }
    }

    pub fn plan_three_way_merge(
        &mut self,
        local: LocalLogin,
        shared: MirrorLogin,
        upstream: Login,
        upstream_time: ServerTimestamp,
        server_now: ServerTimestamp,
    ) {
        let local_age = SystemTime::now()
            .duration_since(local.local_modified)
            .unwrap_or_default();
        let remote_age = server_now.duration_since(upstream_time).unwrap_or_default();

        let local_delta = local.login.delta(&shared.login);
        let upstream_delta = upstream.delta(&shared.login);

        let merged_delta = local_delta.merge(upstream_delta, remote_age < local_age);

        // Update mirror to upstream
        self.mirror_updates
            .push((upstream, upstream_time.as_millis() as i64));
        let mut new = shared;

        new.login.apply_delta(merged_delta);
        new.server_modified = upstream_time;
        self.local_updates.push(new);
    }

    pub fn plan_delete(&mut self, id: Guid) {
        self.delete_local.push(id.clone());
        self.delete_mirror.push(id);
    }

    pub fn plan_mirror_update(&mut self, login: Login, time: ServerTimestamp) {
        self.mirror_updates.push((login, time.as_millis() as i64));
    }

    pub fn plan_mirror_insert(&mut self, login: Login, time: ServerTimestamp, is_override: bool) {
        self.mirror_inserts
            .push((login, time.as_millis() as i64, is_override));
    }

    fn perform_deletes(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        sql_support::each_chunk(&self.delete_local, |chunk, _| -> Result<()> {
            conn.execute(
                &format!(
                    "DELETE FROM loginsL WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            scope.err_if_interrupted()?;
            Ok(())
        })?;

        sql_support::each_chunk(&self.delete_mirror, |chunk, _| {
            conn.execute(
                &format!(
                    "DELETE FROM loginsM WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            Ok(())
        })
    }

    // These aren't batched but probably should be.
    fn perform_mirror_updates(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        let sql = "
            UPDATE loginsM
            SET server_modified = :server_modified,
                httpRealm       = :http_realm,
                formSubmitURL   = :form_submit_url,
                usernameField   = :username_field,
                passwordField   = :password_field,
                passwordEnc     = :password_enc,
                hostname        = :hostname,
                usernameEnc     = :username_enc,
                -- Avoid zeroes if the remote has been overwritten by an older client.
                timesUsed           = coalesce(nullif(:times_used,            0), timesUsed),
                timeLastUsed        = coalesce(nullif(:time_last_used,        0), timeLastUsed),
                timePasswordChanged = coalesce(nullif(:time_password_changed, 0), timePasswordChanged),
                timeCreated         = coalesce(nullif(:time_created,          0), timeCreated)
            WHERE guid = :guid
        ";
        let mut stmt = conn.prepare_cached(sql)?;
        for (login, timestamp) in &self.mirror_updates {
            log::trace!("Updating mirror {:?}", login.guid_str());
            stmt.execute_named(named_params! {
                ":server_modified": *timestamp,
                ":http_realm": login.http_realm,
                ":form_submit_url": login.form_action_origin,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":password_enc": login.password_enc,
                ":hostname": login.origin,
                ":username_enc": login.username_enc,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                ":time_created": login.time_created,
                ":guid": login.guid_str(),
            })?;
            scope.err_if_interrupted()?;
        }
        Ok(())
    }

    fn perform_mirror_inserts(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        let sql = "
            INSERT OR IGNORE INTO loginsM (
                is_overridden,
                server_modified,

                httpRealm,
                formSubmitURL,
                usernameField,
                passwordField,
                passwordEnc,
                hostname,
                usernameEnc,

                timesUsed,
                timeLastUsed,
                timePasswordChanged,
                timeCreated,

                guid
            ) VALUES (
                :is_overridden,
                :server_modified,

                :http_realm,
                :form_submit_url,
                :username_field,
                :password_field,
                :password_enc,
                :hostname,
                :username_enc,

                :times_used,
                :time_last_used,
                :time_password_changed,
                :time_created,

                :guid
            )";
        let mut stmt = conn.prepare_cached(&sql)?;

        for (login, timestamp, is_overridden) in &self.mirror_inserts {
            log::trace!("Inserting mirror {:?}", login.guid_str());
            stmt.execute_named(named_params! {
                ":is_overridden": *is_overridden,
                ":server_modified": *timestamp,
                ":http_realm": login.http_realm,
                ":form_submit_url": login.form_action_origin,
                ":username_field": login.username_field,
                ":password_field": login.password_field,
                ":password_enc": login.password_enc,
                ":hostname": login.origin,
                ":username_enc": login.username_enc,
                ":times_used": login.times_used,
                ":time_last_used": login.time_last_used,
                ":time_password_changed": login.time_password_changed,
                ":time_created": login.time_created,
                ":guid": login.guid_str(),
            })?;
            scope.err_if_interrupted()?;
        }
        Ok(())
    }

    fn perform_local_updates(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        let sql = format!(
            "UPDATE loginsL
             SET local_modified      = :local_modified,
                 httpRealm           = :http_realm,
                 formSubmitURL       = :form_submit_url,
                 usernameField       = :username_field,
                 passwordField       = :password_field,
                 timeLastUsed        = :time_last_used,
                 timePasswordChanged = :time_password_changed,
                 timesUsed           = :times_used,
                 passwordEnc         = :password_enc,
                 hostname            = :hostname,
                 usernameEnc         = :username_enc,
                 sync_status         = {changed}
             WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        // XXX OutgoingChangeset should no longer have timestamp.
        let local_ms: i64 = util::system_time_ms_i64(SystemTime::now());
        for l in &self.local_updates {
            log::trace!("Updating local {:?}", l.guid_str());
            stmt.execute_named(named_params! {
                ":local_modified": local_ms,
                ":http_realm": l.login.http_realm,
                ":form_submit_url": l.login.form_action_origin,
                ":username_field": l.login.username_field,
                ":password_field": l.login.password_field,
                ":password_enc": l.login.password_enc,
                ":hostname": l.login.origin,
                ":username_enc": l.login.username_enc,
                ":time_last_used": l.login.time_last_used,
                ":time_password_changed": l.login.time_password_changed,
                ":times_used": l.login.times_used,
                ":guid": l.guid_str(),
            })?;
            scope.err_if_interrupted()?;
        }
        Ok(())
    }

    pub fn execute(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        log::debug!("UpdatePlan: deleting records...");
        self.perform_deletes(conn, scope)?;
        log::debug!("UpdatePlan: Updating existing mirror records...");
        self.perform_mirror_updates(conn, scope)?;
        log::debug!("UpdatePlan: Inserting new mirror records...");
        self.perform_mirror_inserts(conn, scope)?;
        log::debug!("UpdatePlan: Updating reconciled local records...");
        self.perform_local_updates(conn, scope)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::{
        check_local_login, check_mirror_login, get_local_guids, get_mirror_guids,
        get_server_modified, insert_login,
    };
    use crate::db::LoginDb;
    use crate::login::test_utils::login;

    #[test]
    fn test_deletes() {
        let db = LoginDb::open_in_memory().unwrap();
        insert_login(&db, "login1", Some("password"), Some("password"));
        insert_login(&db, "login2", Some("password"), Some("password"));
        insert_login(&db, "login3", Some("password"), Some("password"));
        insert_login(&db, "login4", Some("password"), Some("password"));

        UpdatePlan {
            delete_mirror: vec![Guid::new("login1"), Guid::new("login2")],
            delete_local: vec![Guid::new("login2"), Guid::new("login3")],
            ..UpdatePlan::default()
        }
        .execute(&db, &db.begin_interrupt_scope())
        .unwrap();

        assert_eq!(get_local_guids(&db), vec!["login1", "login4"]);
        assert_eq!(get_mirror_guids(&db), vec!["login3", "login4"]);
    }

    #[test]
    fn test_mirror_updates() {
        let db = LoginDb::open_in_memory().unwrap();
        insert_login(&db, "unchanged", None, Some("password"));
        insert_login(&db, "changed", None, Some("password"));
        insert_login(
            &db,
            "changed2",
            Some("new-local-password"),
            Some("password"),
        );
        let initial_modified = get_server_modified(&db, "unchanged");

        UpdatePlan {
            mirror_updates: vec![
                (login("changed", "new-password"), 20000),
                (login("changed2", "new-password2"), 21000),
            ],
            ..UpdatePlan::default()
        }
        .execute(&db, &db.begin_interrupt_scope())
        .unwrap();
        check_mirror_login(&db, "unchanged", "password", initial_modified, false);
        check_mirror_login(&db, "changed", "new-password", 20000, false);
        check_mirror_login(&db, "changed2", "new-password2", 21000, true);
    }

    #[test]
    fn test_mirror_inserts() {
        let db = LoginDb::open_in_memory().unwrap();
        UpdatePlan {
            mirror_inserts: vec![
                (login("login1", "new-password"), 20000, false),
                (login("login2", "new-password2"), 21000, true),
            ],
            ..UpdatePlan::default()
        }
        .execute(&db, &db.begin_interrupt_scope())
        .unwrap();
        check_mirror_login(&db, "login1", "new-password", 20000, false);
        check_mirror_login(&db, "login2", "new-password2", 21000, true);
    }

    #[test]
    fn test_local_updates() {
        let db = LoginDb::open_in_memory().unwrap();
        insert_login(&db, "login", Some("password"), Some("password"));
        let before_update = util::system_time_ms_i64(SystemTime::now());

        UpdatePlan {
            local_updates: vec![MirrorLogin {
                login: login("login", "new-password"),
                server_modified: ServerTimestamp(10000),
                is_overridden: false,
            }],
            ..UpdatePlan::default()
        }
        .execute(&db, &db.begin_interrupt_scope())
        .unwrap();
        check_local_login(&db, "login", "new-password", before_update);
    }
}
