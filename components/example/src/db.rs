/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Database handling
//!
//! Components typically use SQLite to handle their storage needs.  Databases are opened in `WAL`
//! mode, which essentially allows for multiple readers, but only one writer at a time.
//! (https://www.sqlite.org/wal.html).  The `Databases` struct is responsible for managing
//! connections and avoids `SQLITE_BUSY` errors.
//!
//! SQLite is typically very fast, but some queries can be slow and operations that run many
//! queries can be slow even if each individual query is fast. The `interrupt_support` crate can
//! help deal with this, by giving you ways to interrupt in-progress database operations.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
// Use the parking_lot Mutex type.  It's faster and more ergonomic than the std one.
use parking_lot::Mutex;
use rusqlite::Connection;
use sql_support::open_database::{open_database_with_flags, read_only_flags, read_write_flags};

use crate::schema::ExampleComponentConnectionInitializer;

use crate::Result;

/// Stores the read and write database connections
///
/// This setup is a good default for new components.  It has decent concurrency, since the reader
/// and writer can work at the same time.  A single connection or a pool of reader connections can
/// also work well and don't have many gotchas.
///
/// Allowing multiple active write connections can cause issues.  If both of them write at the same
/// time, one may receive an `SQLITE_BUSY`.  If you need multiple write connections, take a look at
/// `components/places/doc/sql_concurrency.md` and reach out on the `#app-storage` slack channel
/// for guidance.
pub struct Databases {
    reader: DatabaseConnection,
    writer: DatabaseConnection,
}

impl Databases {
    pub fn new(path: &str) -> Result<Self> {
        crate::error::trace!("Opening database: {path}");
        // Open the write connection first, since it might need to run migrations
        let writer = DatabaseConnection::new_writer(path)?;
        let reader = DatabaseConnection::new_reader(path)?;
        Ok(Self { reader, writer })
    }

    /// Interrupt both connections
    ///
    /// Typical usage: The app is shutting down and wants interrupt all in-progress operations.
    pub fn interrupt_all(&self) {
        self.reader.interrupt_handle.interrupt();
        self.writer.interrupt_handle.interrupt();
    }

    /// Interrupt the reader connection only
    ///
    /// Typical usage: The app is showing a text-box and searching for matching lists when the user
    /// types a new character.  Before running a new query, it calls this method to interrupt any
    /// previously running queries.
    pub fn interrupt_readers(&self) {
        self.reader.interrupt_handle.interrupt();
    }

    /// Perform a read operation on the database
    ///
    /// This calls the supplied closure with a `&Dao`, which allows read methods to be called.
    pub fn read<T>(&self, op: impl FnOnce(&Dao) -> Result<T>) -> Result<T> {
        let conn = self.reader.conn.lock();
        let interrupt_scope = self.reader.interrupt_handle.begin_interrupt_scope()?;
        let dao = Dao {
            conn: &conn,
            interrupt_scope,
        };
        op(&dao)
    }

    /// Perform a write operation on the database
    ///
    /// This calls the supplied closure with a `&mut Dao`, which allows both read and write methods
    /// to be called.
    ///
    /// It also begins a transaction for the write which will be automatically committed if the
    /// operation is successful.
    pub fn write<T>(&self, op: impl FnOnce(&mut Dao) -> Result<T>) -> Result<T> {
        let mut conn = self.writer.conn.lock();
        let interrupt_scope = self.writer.interrupt_handle.begin_interrupt_scope()?;
        let tx = conn.transaction()?;
        let mut dao = Dao {
            conn: &tx,
            interrupt_scope,
        };
        match op(&mut dao) {
            Ok(val) => {
                tx.commit()?;
                Ok(val)
            }
            Err(e) => Err(e),
        }
    }

    // Note: See the `write_scope` method in `suggest/src/db.rs` for another fairly common
    // operation.  `write_scope` is like `write`, except it allows the operation to create and commit
    // multiple transactions.  There's still a single `SqlInterruptScope` that controls the entire
    // operation.
}

/// A single connection to the database
pub struct DatabaseConnection {
    /// The SQLite connection is stored inside a mutex to serialize database operations
    conn: Mutex<Connection>,
    /// `SqlInterruptHandle` is used to interrupt a database operation.
    ///
    /// Calling [SqlInterruptHandle::interrupt] will interrupt any in-progress query and flag all
    /// live `SqlInterruptScope` instances as interrupted.  See [Dao::err_if_interrupted] details
    /// on how to check for interruption in your code.
    ///
    /// This is outside the mutex because we want to interrupt while the connection mutex is being
    /// held by some operation.
    interrupt_handle: Arc<SqlInterruptHandle>,
}

impl DatabaseConnection {
    fn new_reader(path: &str) -> Result<Self> {
        let conn = open_database_with_flags(
            // Path to the sqlite file, this is usually given by the consumer application.
            path,
            // Sqlite open flags, you probably want either `read_only_flags()` or `read_write_flags()`
            read_only_flags(),
            // Initializer, this sets up the database schema and runs any pending migrations.
            // See `schema.rs` for details.
            &ExampleComponentConnectionInitializer,
        )?;
        let interrupt_handle = SqlInterruptHandle::new(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
            interrupt_handle: Arc::new(interrupt_handle),
        })
    }

    fn new_writer(path: &str) -> Result<Self> {
        let conn = open_database_with_flags(
            path,
            read_write_flags(),
            &ExampleComponentConnectionInitializer,
        )?;
        let interrupt_handle = SqlInterruptHandle::new(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
            interrupt_handle: Arc::new(interrupt_handle),
        })
    }
}

/// Data Access Object
///
/// This stores a borrowed SQLite connection alongside an interrupt_scope.
///
/// This is where we define methods that access the DB.  Read methods use a regular self-reference
/// (`&self`) while write methods use a mutable one (`&mut self`).
pub struct Dao<'a> {
    conn: &'a Connection,
    interrupt_scope: SqlInterruptScope,
}

impl Dao<'_> {
    // Check the interrupt scope, returning `Error::Interrupted` if some other thread has
    // interrupted our operation.
    //
    // If you have a long-running operation, call this at the start of each loop to ensure that the
    // operation can be interrupted.  What you want to avoid another thread calling
    // `SqlInterruptHandle::interrupt` while you're doing non-database work in the loop, then never
    // checking the interrupted flag.
    fn err_if_interrupted(&self) -> Result<()> {
        Ok(self.interrupt_scope.err_if_interrupted()?)
    }

    /// Get all lists in the database
    pub fn get_lists(&self) -> Result<Vec<String>> {
        // Use `prepare` to create a SQLite statement that can then be executed.
        let mut stmt = self.conn.prepare("SELECT name FROM todo_list")?;
        // Use `query_and_then` to run the query.
        //
        // The 1st arg is for query parameters which we don't need for this query.  See the next
        // method for how these work.  The 2nd arg is a closure to extract data from the rows.
        let result = stmt.query_and_then((), |rows| {
            // Use `row.get()` to fetch data from the row, passing in the column index..  This can
            // return any type of data supported by `rusqlite`.  Give Rust some type annotations so
            // that it can resolve any ambiguity.
            let name: String = rows.get(0)?;
            Ok(name)
        })?;
        // Use `collect` to collect the rows into a Vec.
        // Note: for lifetime reasons, this must be a separate statement -- you can't just add
        // `.collect()` to the last line.
        result.collect()
    }

    /// Get lists who's name starts with `query`
    pub fn find_lists(&self, query: &str) -> Result<Vec<String>> {
        // Use the `?` char as a placeholder for query parameters.
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM todo_list WHERE name LIKE ? || '%'")?;
        // This query requires a parameter for the `name` WHERE clause.  A Rust tuple is the
        // simplest way to pass in parameters and works in almost all cases.  If you run into a
        // case where it doesn't work, take a look at the
        // [named_params macro](https://docs.rs/rusqlite/latest/rusqlite/macro.named_params.html).
        let result = stmt.query_and_then((query,), |row| {
            // Here's another way to supply the type annotations
            Ok(row.get::<_, String>(0)?)
        })?;
        result.collect()
    }

    /// Get an item in a list
    pub fn get_list_item(&self, list_name: &str, item_name: &str) -> Result<SavedTodoItem> {
        let sql = "
              SELECT t.id, t.name, t.description, t.last_modified, t.completed, t.url
              FROM todo t
              JOIN todo_list l
                  ON t.list_id = l.id
              WHERE l.name = ? AND t.name = ?
              ";
        self.conn
            .query_row_and_then(sql, (list_name, item_name), SavedTodoItem::from_row)
    }

    /// Get all items in a list
    pub fn get_list_items(&self, list_name: &str) -> Result<Vec<SavedTodoItem>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT t.id, t.name, t.description, t.last_modified, t.completed, t.url
            FROM todo t
            JOIN todo_list l
                ON t.list_id = l.id
            WHERE l.name = ?
            ",
        )?;
        let result = stmt.query_and_then((list_name,), SavedTodoItem::from_row)?;
        result.collect()
    }

    /// Create a new list
    pub fn create_list(&self, name: &str) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("INSERT INTO todo_list(name) VALUES (?)")?;
        // Use `execute` to execute an INSERT/UPDATE statement.
        stmt.execute((name,))?;
        Ok(())
    }

    /// Delete a list
    pub fn delete_list(&self, name: &str) -> Result<()> {
        let mut stmt = self.conn.prepare("DELETE FROM todo_list WHERE name = ?")?;
        stmt.execute((name,))?;
        Ok(())
    }

    /// Create a new list item
    pub fn add_item(&self, list_name: &str, item: TodoItem) -> Result<SavedTodoItem> {
        let last_modified = current_timestamp();
        let mut stmt = self.conn.prepare(
            "INSERT INTO todo(list_id, name, description, last_modified, completed, url)
            VALUES (
                (SELECT id FROM todo_list WHERE name = ?),
                ?,
                ?,
                ?,
                ?,
                ?
            )
            ",
        )?;
        stmt.execute((
            list_name,
            &item.name,
            &item.description,
            last_modified,
            item.completed,
            &item.url,
        ))?;
        Ok(SavedTodoItem {
            id: self.conn.last_insert_rowid(),
            last_modified,
            item,
        })
    }

    /// Bulk-create a new list item
    pub fn add_items(&self, list_name: &str, items: Vec<TodoItem>) -> Result<Vec<SavedTodoItem>> {
        let last_modified = current_timestamp();
        // The `prepare` call is especially important for operations like this, we don't want to
        // waste time having SQLite re-interpret the SQL on each iteration.
        //
        // On the topic of `prepare`, it eliminates almost all the overhead of making queries to
        // SQLite.  It's usually fine to run execute subqueries in a loop as long as the statement
        // is prepared outside of that look.
        let mut stmt = self.conn.prepare(
            "INSERT INTO todo(list_id, name, description, last_modified, completed, url)
            VALUES (
                (SELECT id FROM todo_list WHERE name = ?),
                ?,
                ?,
                ?,
                ?,
                ?
            )
            ",
        )?;
        let mut saved_items = vec![];
        for item in items {
            // When running many statements in a loop, make sure to check for interruption on each
            // iteration.  Otherwise, if an interrupt happens outside of a SQL query it won't have
            // any effect.
            self.err_if_interrupted()?;
            stmt.execute((
                list_name,
                &item.name,
                &item.description,
                last_modified,
                item.completed,
                &item.url,
            ))?;
            saved_items.push(SavedTodoItem {
                id: self.conn.last_insert_rowid(),
                last_modified,
                item,
            })
        }
        Ok(saved_items)
    }

    /// Update a list item
    pub fn update_item(&self, saved_item: &SavedTodoItem) -> Result<()> {
        // You don't always have to prepare statements ahead of time, this method uses
        // `Connection::execute` to execute them directly.
        self.conn.execute(
            "
            UPDATE todo
            SET name=?,
                description=?,
                completed=?,
                url=?
            WHERE id=?
            ",
            (
                &saved_item.item.name,
                &saved_item.item.description,
                &saved_item.item.completed,
                &saved_item.item.url,
                &saved_item.id,
            ),
        )?;
        Ok(())
    }

    /// Delete list item
    pub fn delete_item(&self, saved_item: SavedTodoItem) -> Result<()> {
        self.conn
            .execute("DELETE FROM todo WHERE id=?", (saved_item.id,))?;
        Ok(())
    }
}

/// Todo item
///
/// Notes:
///  * It's often a good idea to create structs for your database rows.
///  * This needs to derive `uniffi::Record` since it's a struct that's returned to the consumer as
///    part of the public API
#[derive(Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct TodoItem {
    pub name: String,
    pub description: String,
    pub url: String,
    pub completed: bool,
}

/// Todo item that's been saved to the db
///
/// How should you distinguish between items that are saved to the db or not?
///
/// This example component uses an onion-style struct, where the saved version of the struct stores
/// the non-saved one plus rows that are only present for saved items.  Another possibility would
/// be to have a single struct with an `Option<SavedRowData>` field.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct SavedTodoItem {
    pub id: i64,
    pub last_modified: u64,
    pub item: TodoItem,
}

impl TodoItem {
    fn from_row(row: &rusqlite::Row<'_>) -> Result<Self> {
        Ok(TodoItem {
            name: row.get("name")?,
            description: row.get("description")?,
            completed: row.get("completed")?,
            url: row.get("url")?,
        })
    }
}

impl SavedTodoItem {
    fn from_row(row: &rusqlite::Row<'_>) -> Result<Self> {
        Ok(SavedTodoItem {
            id: row.get("id")?,
            last_modified: row.get("last_modified")?,
            item: TodoItem::from_row(row)?,
        })
    }
}

fn current_timestamp() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        // Let's hope user's clocks aren't set before 1970, but if they are use `0` as the
        // timestamp.
        .unwrap_or_else(|_| std::time::Duration::default())
        .as_secs()
}

#[cfg(test)]
mod test {
    use super::*;
    use sql_support::open_database::unique_in_memory_db_path;

    /// Memory-only connections are a good way to test DB code
    ///
    /// Use the `cache=[name]` so that the reader and writer dbs share the same in-memory
    /// connection.
    impl Databases {
        fn new_memory() -> Self {
            Self::new(&unique_in_memory_db_path()).unwrap()
        }
    }

    #[test]
    fn test_lists() {
        let dbs = Databases::new_memory();
        assert_eq!(
            dbs.read(|dao| dao.get_lists()).unwrap(),
            Vec::<String>::new()
        );

        dbs.write(|dao| {
            dao.create_list("foo")?;
            dao.create_list("bar")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            dbs.read(|dao| dao.get_lists()).unwrap(),
            vec!["foo".to_string(), "bar".to_string()]
        );
        assert_eq!(
            dbs.read(|dao| dao.find_lists("fo")).unwrap(),
            vec!["foo".to_string()]
        );
    }

    #[test]
    fn test_create_items() {
        let dbs = Databases::new_memory();
        dbs.write(|dao| {
            dao.create_list("foo")?;
            dao.create_list("bar")?;
            dao.add_item(
                "foo",
                TodoItem {
                    name: "laundry".into(),
                    description: "Wash clothes".into(),
                    ..TodoItem::default()
                },
            )?;
            dao.add_item(
                "foo",
                TodoItem {
                    name: "dishes".into(),
                    url: "http://example.com/dishes".into(),
                    ..TodoItem::default()
                },
            )?;
            dao.add_item(
                "bar",
                TodoItem {
                    name: "taxes".into(),
                    completed: true,
                    ..TodoItem::default()
                },
            )?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            dbs.read(|dao| dao.get_list_items("foo"))
                .unwrap()
                .into_iter()
                .map(|saved| saved.item)
                .collect::<Vec<_>>(),
            vec![
                TodoItem {
                    name: "laundry".into(),
                    description: "Wash clothes".into(),
                    ..TodoItem::default()
                },
                TodoItem {
                    name: "dishes".into(),
                    url: "http://example.com/dishes".into(),
                    ..TodoItem::default()
                },
            ]
        );
        assert_eq!(
            dbs.read(|dao| dao.get_list_items("bar"))
                .unwrap()
                .into_iter()
                .map(|saved| saved.item)
                .collect::<Vec<_>>(),
            vec![TodoItem {
                name: "taxes".into(),
                completed: true,
                ..TodoItem::default()
            },]
        );
    }

    #[test]
    fn test_update_delete_items() {
        let dbs = Databases::new_memory();
        dbs.write(|dao| {
            dao.create_list("foo")?;
            let laundry = dao.add_item(
                "foo",
                TodoItem {
                    name: "laundry".into(),
                    ..TodoItem::default()
                },
            )?;
            let mut dishes = dao.add_item(
                "foo",
                TodoItem {
                    name: "dishes".into(),
                    ..TodoItem::default()
                },
            )?;
            dishes.item.name = "new-title".into();
            dishes.item.description = "updated-desc".into();
            dishes.item.url = "http://example.com/item".into();
            dishes.item.completed = true;
            dao.update_item(&dishes)?;
            dao.delete_item(laundry)?;
            Ok(())
        })
        .unwrap();

        let items = dbs
            .read(|dao| {
                Ok(dao
                    .get_list_items("foo")?
                    .into_iter()
                    .map(|saved| saved.item)
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert_eq!(
            items,
            vec![TodoItem {
                name: "new-title".into(),
                description: "updated-desc".into(),
                url: "http://example.com/item".into(),
                completed: true,
            }]
        );
    }

    #[test]
    fn test_delete_list() {
        let dbs = Databases::new_memory();
        dbs.write(|dao| {
            dao.create_list("baz")?;
            dao.add_item(
                "baz",
                TodoItem {
                    name: "item1".into(),
                    ..TodoItem::default()
                },
            )?;
            dao.add_item(
                "baz",
                TodoItem {
                    name: "item2".into(),
                    ..TodoItem::default()
                },
            )?;
            dao.delete_list("baz")?;
            Ok(())
        })
        .unwrap();

        assert!(dbs.read(|dao| dao.get_lists()).unwrap().is_empty());
        assert!(dbs.read(|dao| dao.find_lists("baz")).unwrap().is_empty());
        assert!(dbs
            .read(|dao| dao.get_list_items("baz"))
            .unwrap()
            .is_empty());
    }
}
