use rusqlite::Connection;
use sql_support::ConnExt;

use push_errors::Result;

const VERSION: i64 = 1;

const CREATE_TABLE_PUSH_SQL: &'static str = include_str!("schema.sql");

pub const COMMON_COLS: &'static str = "
    uaid,
    channel_id,
    endpoint,
    scope,
    key,
    ctime,
    app_server_key,
    native_id
";

pub fn init(db: &Connection) -> Result<()> {
    let user_version = db.query_one::<i64>("PRAGMA user_version")?;
    if user_version == 0 {
        create(db)?;
    } else if user_version != VERSION {
        if user_version < VERSION {
            upgrade(db, user_version)?;
        } else {
            log::warn!(
                "Loaded future schema version {} (we only understand version {}). \
                 Optimisitically ",
                user_version,
                VERSION
            )
        }
    }
    Ok(())
}

fn upgrade(_db: &Connection, from: i64) -> Result<()> {
    log::debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    panic!("sorry, no upgrades yet - delete your db!");
}

pub fn create(db: &Connection) -> Result<()> {
    log::debug!("Creating schema");
    db.execute_all(&[
        CREATE_TABLE_PUSH_SQL,
        &format!("PRAGMA user_version = {version}", version = VERSION),
    ])?;

    Ok(())
}
