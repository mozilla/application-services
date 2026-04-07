/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::Connection;
use sql_support::ConnExt;

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BreachAlertDismissal {
    /// The HIBP breach name. HIBP calls this a "name" rather than an ID, but it is unique
    /// and serves as the breach's stable identifier.
    pub breach_name: String,
    /// Unix timestamp in milliseconds of when the breach alert was last dismissed.
    pub time_dismissed: i64,
}

pub fn get_breach_alert_dismissals(
    conn: &Connection,
    breach_names: &[String],
) -> Result<Vec<BreachAlertDismissal>> {
    if breach_names.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<&str> = breach_names.iter().map(|_| "?").collect();
    let sql = format!(
        "SELECT breach_name, time_dismissed FROM breach_alert_dismissals WHERE breach_name IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.conn().prepare(&sql)?;
    let params: Vec<&dyn rusqlite::types::ToSql> = breach_names
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok(BreachAlertDismissal {
            breach_name: row.get(0)?,
            time_dismissed: row.get(1)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::from)
}

pub fn set_breach_alert_dismissals(
    conn: &Connection,
    dismissals: &[BreachAlertDismissal],
) -> Result<()> {
    if dismissals.is_empty() {
        return Ok(());
    }
    let placeholders: Vec<&str> = dismissals.iter().map(|_| "(?, ?)").collect();
    let sql = format!(
        "INSERT OR REPLACE INTO breach_alert_dismissals (breach_name, time_dismissed) VALUES {}",
        placeholders.join(", ")
    );
    let mut params: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(dismissals.len() * 2);
    for d in dismissals {
        params.push(&d.breach_name);
        params.push(&d.time_dismissed);
    }
    conn.conn().execute(&sql, params.as_slice())?;
    Ok(())
}

pub fn clear_breach_alert_dismissals(conn: &Connection, breach_names: &[String]) -> Result<()> {
    if breach_names.is_empty() {
        return Ok(());
    }
    let placeholders: Vec<&str> = breach_names.iter().map(|_| "?").collect();
    let sql = format!(
        "DELETE FROM breach_alert_dismissals WHERE breach_name IN ({})",
        placeholders.join(",")
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = breach_names
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    conn.conn().execute(&sql, params.as_slice())?;
    Ok(())
}

pub fn clear_all_breach_alert_dismissals(conn: &Connection) -> Result<()> {
    conn.conn()
        .execute("DELETE FROM breach_alert_dismissals", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    #[test]
    fn test_set_and_get() -> Result<()> {
        let db = new_mem_db();
        let conn = db.get_connection()?;

        let dismissals = vec![
            BreachAlertDismissal {
                breach_name: "breach-1".into(),
                time_dismissed: 1000,
            },
            BreachAlertDismissal {
                breach_name: "breach-2".into(),
                time_dismissed: 2000,
            },
        ];
        set_breach_alert_dismissals(conn, &dismissals)?;

        let result = get_breach_alert_dismissals(
            conn,
            &["breach-1".into(), "breach-2".into(), "breach-3".into()],
        )?;
        assert_eq!(result.len(), 2);
        assert!(result.contains(&BreachAlertDismissal {
            breach_name: "breach-1".into(),
            time_dismissed: 1000
        }));
        assert!(result.contains(&BreachAlertDismissal {
            breach_name: "breach-2".into(),
            time_dismissed: 2000
        }));
        Ok(())
    }

    #[test]
    fn test_set_updates_existing() -> Result<()> {
        let db = new_mem_db();
        let conn = db.get_connection()?;

        set_breach_alert_dismissals(
            conn,
            &[BreachAlertDismissal {
                breach_name: "breach-1".into(),
                time_dismissed: 1000,
            }],
        )?;
        set_breach_alert_dismissals(
            conn,
            &[BreachAlertDismissal {
                breach_name: "breach-1".into(),
                time_dismissed: 2000,
            }],
        )?;

        let result = get_breach_alert_dismissals(conn, &["breach-1".into()])?;
        assert_eq!(
            result,
            vec![BreachAlertDismissal {
                breach_name: "breach-1".into(),
                time_dismissed: 2000
            }]
        );
        Ok(())
    }

    #[test]
    fn test_clear_specific() -> Result<()> {
        let db = new_mem_db();
        let conn = db.get_connection()?;

        set_breach_alert_dismissals(
            conn,
            &[
                BreachAlertDismissal {
                    breach_name: "breach-1".into(),
                    time_dismissed: 1000,
                },
                BreachAlertDismissal {
                    breach_name: "breach-2".into(),
                    time_dismissed: 2000,
                },
            ],
        )?;
        clear_breach_alert_dismissals(conn, &["breach-1".into()])?;

        let result = get_breach_alert_dismissals(conn, &["breach-1".into(), "breach-2".into()])?;
        assert_eq!(
            result,
            vec![BreachAlertDismissal {
                breach_name: "breach-2".into(),
                time_dismissed: 2000
            }]
        );
        Ok(())
    }

    #[test]
    fn test_clear_all() -> Result<()> {
        let db = new_mem_db();
        let conn = db.get_connection()?;

        set_breach_alert_dismissals(
            conn,
            &[
                BreachAlertDismissal {
                    breach_name: "breach-1".into(),
                    time_dismissed: 1000,
                },
                BreachAlertDismissal {
                    breach_name: "breach-2".into(),
                    time_dismissed: 2000,
                },
            ],
        )?;
        clear_all_breach_alert_dismissals(conn)?;

        let result = get_breach_alert_dismissals(conn, &["breach-1".into(), "breach-2".into()])?;
        assert!(result.is_empty());
        Ok(())
    }

    #[test]
    fn test_empty_inputs() -> Result<()> {
        let db = new_mem_db();
        let conn = db.get_connection()?;

        assert_eq!(get_breach_alert_dismissals(conn, &[])?, vec![]);
        set_breach_alert_dismissals(conn, &[])?;
        clear_breach_alert_dismissals(conn, &[])?;
        Ok(())
    }
}
