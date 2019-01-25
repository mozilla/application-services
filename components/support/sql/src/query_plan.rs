/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{types::ToSql, Connection, Result as SqlResult};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryPlanStep {
    pub select_id: i32,
    pub order: i32,
    pub from: i32,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryPlan {
    pub query: String,
    pub plan: Vec<QueryPlanStep>,
}

impl QueryPlan {
    // TODO: support positional params (it's a pain...)
    pub fn new(conn: &Connection, sql: &str, params: &[(&str, &dyn ToSql)]) -> SqlResult<Self> {
        let plan_sql = format!("EXPLAIN QUERY PLAN {}", sql);
        let mut stmt = conn.prepare(&plan_sql)?;
        let plan = stmt
            .query_and_then_named(params, |row| -> SqlResult<_> {
                Ok(QueryPlanStep {
                    select_id: row.get_checked(0)?,
                    order: row.get_checked(1)?,
                    from: row.get_checked(2)?,
                    detail: row.get_checked(3)?,
                })
            })?
            .collect::<Result<Vec<QueryPlanStep>, _>>()?;
        Ok(QueryPlan {
            query: sql.into(),
            plan,
        })
    }
}

impl std::fmt::Display for QueryPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "### QUERY PLAN")?;
        writeln!(f, "#### SQL:\n{}\n#### PLAN:", self.query)?;
        for step in self.plan.iter() {
            writeln!(
                f,
                "|{}|{}|{}|{}|",
                step.select_id, step.order, step.from, step.detail
            )?;
        }
        writeln!(f, "### END QUERY PLAN")
    }
}

/// Log a query plan if the `log_query_plans` feature is enabled and it hasn't been logged yet.
#[inline]
pub fn maybe_log_plan(_conn: &Connection, _sql: &str, _params: &[(&str, &dyn ToSql)]) {
    // Note: underscores ar needed becasue those go unused if the feature is not turned on.
    #[cfg(feature = "log_query_plans")]
    {
        plan_log::log_plan(_conn, _sql, _params)
    }
}

#[cfg(feature = "log_query_plans")]
mod plan_log {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::Mutex;

    struct PlanLogger {
        seen: HashMap<String, QueryPlan>,
        out: Box<dyn Write + Send>,
    }

    impl PlanLogger {
        fn new() -> Self {
            let out_file = std::env::var("QUERY_PLAN_LOG").unwrap_or_default();
            let output: Box<dyn Write + Send> = if out_file != "" {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(out_file)
                    .expect("QUERY_PLAN_LOG file does not exist!");
                writeln!(
                    file,
                    "\n\n# Query Plan Log starting at time: {:?}\n",
                    std::time::SystemTime::now()
                )
                .expect("Failed to write to plan log file");
                Box::new(file)
            } else {
                println!("QUERY_PLAN_LOG was not set, logging to stdout");
                Box::new(std::io::stdout())
            };
            Self {
                seen: Default::default(),
                out: output,
            }
        }

        fn maybe_log(&mut self, plan: QueryPlan) {
            use std::collections::hash_map::Entry;
            match self.seen.entry(plan.query.clone()) {
                Entry::Occupied(mut o) => {
                    if o.get() == &plan {
                        return;
                    }
                    // Ignore IO failures.
                    let _ = writeln!(self.out, "### QUERY PLAN CHANGED!\n{}", plan);
                    o.insert(plan);
                }
                Entry::Vacant(v) => {
                    let _ = writeln!(self.out, "{}", plan);
                    v.insert(plan);
                }
            }
            let _ = self.out.flush();
        }
    }

    lazy_static::lazy_static! {
        static ref PLAN_LOGGER: Mutex<PlanLogger> = Mutex::new(PlanLogger::new());
    }

    pub fn log_plan(conn: &Connection, sql: &str, params: &[(&str, &dyn ToSql)]) {
        if sql.starts_with("EXPLAIN") {
            return;
        }
        let plan = match QueryPlan::new(conn, sql, params) {
            Ok(plan) => plan,
            Err(e) => {
                // We're usually doing this during tests where logs often arent available
                eprintln!("Failed to get query plan for {}: {}", sql, e);
                return;
            }
        };
        let mut logger = PLAN_LOGGER.lock().unwrap();
        logger.maybe_log(plan);
    }
}
