use rusqlite::Connection;
use sql_support::ConnExt;

use push_errors::Result;

const VERSION: i64 = 1;

// XXX: named "pushapi", maybe push_sub?
const CREATE_TABLE_PUSH_SQL: &'static str = "
CREATE TABLE IF NOT EXISTS push_record (
    channel_id         TEXT     NOT NULL PRIMARY KEY,
    endpoint           TEXT     NOT NULL UNIQUE,
    scope              TEXT     NOT NULL,
    origin_attributes  TEXT     NOT NULL,
    key                TEXT     NOT NULL,
    system_record      TINYINT  NOT NULL,
    recent_message_ids TEXT     NOT NULL,
    push_count         SMALLINT NOT NULL,
    last_push          INTEGER  NOT NULL,
    ctime              INTEGER  NOT NULL,
    quota              TINYINT  NOT NULL,
    app_server_key     TEXT,
    native_id          TEXT
);

-- index to fetch records based on endpoints. used by unregister
CREATE INDEX idx_endpoint ON push_record (endpoint);

-- index to fetch records by identifiers. In the current security
-- model, the originAttributes distinguish between different 'apps' on
-- the same origin. Since ServiceWorkers are same-origin to the scope
-- they are registered for, the attributes and scope are enough to
-- reconstruct a valid principal.
CREATE UNIQUE INDEX idx_identifiers ON push_record (scope, origin_attributes);
CREATE INDEX idx_origin_attributes ON push_record (origin_attributes);
";

pub const COMMON_COLS: &'static str = "
    channel_id,
    endpoint,
    scope,
    origin_attributes,
    key,
    system_record,
    recent_message_ids,
    push_count,
    last_push,
    ctime,
    quota,
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
