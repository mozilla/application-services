/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::login::{LocalLogin, Login, MirrorLogin, SyncStatus};
use crate::util;
use rusqlite::{types::ToSql, Connection};
use sql_support::SqlInterruptScope;
use std::time::SystemTime;
use sync15::ServerTimestamp;

#[derive(Default, Debug, Clone)]
pub(crate) struct UpdatePlan {
    pub delete_mirror: Vec<String>,
    pub delete_local: Vec<String>,
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
            self.delete_local.push(local.id.to_string());
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

    pub fn plan_delete(&mut self, id: String) {
        self.delete_local.push(id.to_string());
        self.delete_mirror.push(id.to_string());
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
                password        = :password,
                hostname        = :hostname,
                username        = :username,
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
            stmt.execute_named(&[
                (":server_modified", timestamp as &dyn ToSql),
                (":http_realm", &login.http_realm as &dyn ToSql),
                (":form_submit_url", &login.form_submit_url as &dyn ToSql),
                (":username_field", &login.username_field as &dyn ToSql),
                (":password_field", &login.password_field as &dyn ToSql),
                (":password", &login.password as &dyn ToSql),
                (":hostname", &login.hostname as &dyn ToSql),
                (":username", &login.username as &dyn ToSql),
                (":times_used", &login.times_used as &dyn ToSql),
                (":time_last_used", &login.time_last_used as &dyn ToSql),
                (
                    ":time_password_changed",
                    &login.time_password_changed as &dyn ToSql,
                ),
                (":time_created", &login.time_created as &dyn ToSql),
                (":guid", &login.guid_str() as &dyn ToSql),
            ])?;
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
                password,
                hostname,
                username,

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
                :password,
                :hostname,
                :username,

                :times_used,
                :time_last_used,
                :time_password_changed,
                :time_created,

                :guid
            )";
        let mut stmt = conn.prepare_cached(&sql)?;

        for (login, timestamp, is_overridden) in &self.mirror_inserts {
            log::trace!("Inserting mirror {:?}", login.guid_str());
            stmt.execute_named(&[
                (":is_overridden", is_overridden as &dyn ToSql),
                (":server_modified", timestamp as &dyn ToSql),
                (":http_realm", &login.http_realm as &dyn ToSql),
                (":form_submit_url", &login.form_submit_url as &dyn ToSql),
                (":username_field", &login.username_field as &dyn ToSql),
                (":password_field", &login.password_field as &dyn ToSql),
                (":password", &login.password as &dyn ToSql),
                (":hostname", &login.hostname as &dyn ToSql),
                (":username", &login.username as &dyn ToSql),
                (":times_used", &login.times_used as &dyn ToSql),
                (":time_last_used", &login.time_last_used as &dyn ToSql),
                (
                    ":time_password_changed",
                    &login.time_password_changed as &dyn ToSql,
                ),
                (":time_created", &login.time_created as &dyn ToSql),
                (":guid", &login.guid_str() as &dyn ToSql),
            ])?;
            scope.err_if_interrupted()?;
        }
        Ok(())
    }

    fn perform_local_updates(&self, conn: &Connection, scope: &SqlInterruptScope) -> Result<()> {
        let sql = format!(
            "
            UPDATE loginsL
            SET local_modified      = :local_modified,
                httpRealm           = :http_realm,
                formSubmitURL       = :form_submit_url,
                usernameField       = :username_field,
                passwordField       = :password_field,
                timeLastUsed        = :time_last_used,
                timePasswordChanged = :time_password_changed,
                timesUsed           = :times_used,
                password            = :password,
                hostname            = :hostname,
                username            = :username,
                sync_status         = {changed}
            WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        // XXX OutgoingChangeset should no longer have timestamp.
        let local_ms: i64 = util::system_time_ms_i64(SystemTime::now());
        for l in &self.local_updates {
            log::trace!("Updating local {:?}", l.guid_str());
            stmt.execute_named(&[
                (":local_modified", &local_ms as &dyn ToSql),
                (":http_realm", &l.login.http_realm as &dyn ToSql),
                (":form_submit_url", &l.login.form_submit_url as &dyn ToSql),
                (":username_field", &l.login.username_field as &dyn ToSql),
                (":password_field", &l.login.password_field as &dyn ToSql),
                (":password", &l.login.password as &dyn ToSql),
                (":hostname", &l.login.hostname as &dyn ToSql),
                (":username", &l.login.username as &dyn ToSql),
                (":time_last_used", &l.login.time_last_used as &dyn ToSql),
                (
                    ":time_password_changed",
                    &l.login.time_password_changed as &dyn ToSql,
                ),
                (":times_used", &l.login.times_used as &dyn ToSql),
                (":guid", &l.guid_str() as &dyn ToSql),
            ])?;
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
