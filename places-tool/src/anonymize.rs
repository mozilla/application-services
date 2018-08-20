use rand::{self, prelude::*};
use std::collections::HashMap;
use rusqlite::{self, Connection, OpenFlags};
use std::path::PathBuf;
use std::fs;
use failure;


#[derive(Default, Clone, Debug)]
struct StringAnonymizer {
    table: HashMap<String, String>,
}

fn rand_string_of_len(len: usize) -> String {
    let mut rng = thread_rng();
    rng.sample_iter(&rand::distributions::Alphanumeric).take(len).collect()
}

impl StringAnonymizer {
    fn anonymize(&mut self, s: &str) -> String {
        if s.len() == 0 {
            return "".into();
        }
        if let Some(a) = self.table.get(s) {
            return a.clone();
        }
        for i in 0..10 {
            let replacement = rand_string_of_len(s.len());
            // keep trying but force it at the last time
            if self.table.get(&replacement).is_some() && i != 9 {
                continue;
            }

            self.table.insert(s.into(), replacement.clone());
            return replacement;
        }
        unreachable!("Bug in anonymize retry loop");
    }
}

#[derive(Debug, Clone)]
struct TableInfo {
    name: String,
    cols: Vec<String>
}

impl TableInfo {

    fn for_table(name: String, conn: &Connection) -> Result<TableInfo, failure::Error> {
        let stmt = conn.prepare(&format!("SELECT * FROM {}", name))?;
        let cols = stmt.column_names().into_iter().map(|x| x.to_owned()).collect();
        Ok(TableInfo { name, cols })
    }

    fn make_update(&self, updater_fn: &str) -> String {
        let sets = self.cols.iter()
            .map(|col| format!("{} = {}({})", col, updater_fn, col))
            .collect::<Vec<_>>()
            .join(",\n    ");
        format!("UPDATE {}\nSET {}", self.name, sets)
    }
}

fn anonymize(anon_places: &Connection) -> Result<(), failure::Error> {
    {
        let mut anonymizer = StringAnonymizer::default();
        anon_places.create_scalar_function("anonymize", 1, true, move |ctx| {
            let arg = ctx.get::<rusqlite::types::Value>(0)?;
            Ok(match arg {
                rusqlite::types::Value::Text(s) =>
                    rusqlite::types::Value::Text(anonymizer.anonymize(&s)),
                not_text => not_text
            })
        })?;
    }

    let schema = {
        let mut stmt = anon_places.prepare("
            SELECT name FROM sqlite_master
            WHERE type = 'table'
              AND name NOT LIKE 'sqlite_%' -- ('sqlite_sequence', 'sqlite_stat1', 'sqlite_master', anyt)
        ")?;
        let mut rows = stmt.query(&[])?;
        let mut tables = vec![];
        while let Some(row_or_error) = rows.next() {
            tables.push(TableInfo::for_table(row_or_error?.get("name"), &anon_places)?);
        }
        tables
    };

    for info in schema {
        let sql = info.make_update("anonymize");
        debug!("Executing sql:\n{}", sql);
        anon_places.execute(&sql, &[])?;
    }

    debug!("Clearing places url_hash");
    anon_places.execute("UPDATE moz_places SET url_hash = 0", &[])?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct AnonymizePlaces {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
}

impl AnonymizePlaces {

    pub fn run(self) -> Result<(), failure::Error> {
        fs::copy(&self.input_path, &self.output_path)?;
        let anon_places = Connection::open_with_flags(&self.output_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        anonymize(&anon_places)?;
        Ok(())
    }

}



