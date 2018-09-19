/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Roughly, UnifiedComplete.js's stuff

/* XXX - all this is todo 
use super::connection::{Connection};

use rusqlite::{types::{ToSql, FromSql}};

pub struct SearchFrecentParams {
    search_string: String,
}

impl SearchFrecentParams {
    fn named_params(&self) -> &[(&str, &ToSql)] {
        &[(":searchString", &self.search_string)];
    }
}

pub fn searchFrecent(conn: &Connection, params: SearchFrecentParams) {
    // obvs not correct!
    stmt = "SELECT url from moz_places where url like :searchString";
    conn.db.execute_named(stmt, params.named_params());
    Ok(());
}

#[cfg(test)]
mod tests {
}
*/