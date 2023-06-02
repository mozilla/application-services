# Debugging Sql

It can be quite tricky to debug what is going on with sql statement, especially
once the sql gets complicated or many triggers are involved.

The `sql_support` create provides some utilities to help. Note that
these utilities are gated behind a `debug-tools` feature. [The module
provides docstrings, so you should read them before you start](
https://mozilla.github.io/application-services/book/rust-docs/sql_support/debug_tools/index.html).


This document describes how to use these capabilities and we'll use `places`
as an example.

First, we must enable the feature:

```diff
--- a/components/places/Cargo.toml
+++ b/components/places/Cargo.toml
@@ -22,7 +22,7 @@ lazy_static = "1.4"
 url = { version = "2.1", features = ["serde"] }
 percent-encoding = "2.1"
 caseless = "0.2"
-sql-support = { path = "../support/sql" }
+sql-support = { path = "../support/sql", features=["debug-tools"] }
```

and we probably need to make the debug functions available:
```diff
--- a/components/places/src/db/db.rs
+++ b/components/places/src/db/db.rs
@@ -108,6 +108,7 @@ impl ConnectionInitializer for PlacesInitializer {
         ";
         conn.execute_batch(initial_pragmas)?;
         define_functions(conn, self.api_id)?;
+        sql_support::debug_tools::define_debug_functions(conn)?;
```

We now have a Rust function `print_query()` and a SQL function `dbg()` available.

Let's say we were trying to debug a test such as `test_bookmark_tombstone_auto_created`.
We might want to print the entire contents of a table, then instrument a query to check
what the value of a query is. We might end up with a patch something like:
```diff
index 28f19307..225dccbb 100644
--- a/components/places/src/db/schema.rs
+++ b/components/places/src/db/schema.rs
@@ -666,7 +666,8 @@ mod tests {
             [],
         )
         .expect("should insert regular bookmark folder");
-        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'bookmarkguid'", [])
+        sql_support::debug_tools::print_query(&conn, "select * from moz_bookmarks").unwrap();
+        conn.execute("DELETE FROM moz_bookmarks WHERE dbg('CHECKING GUID', guid) = 'bookmarkguid'", [])
             .expect("should delete");
         // should have a tombstone.
         assert_eq!(
```

There are 2 things of note:
* We used the `print_query` function to dump the entire `moz_bookmarks` table before executing the query.
* We instrumented the query to print the `guid` every time sqlite reads a row and compares it against
  a literal.

The output of this test now looks something like:
```
running 1 test
query: select * from moz_bookmarks
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| id | fk   | type | parent | position | title   | dateAdded     | lastModified  | guid         | syncStatus | syncChangeCounter |
+====+======+======+========+==========+=========+===============+===============+==============+============+===================+
| 1  | null | 2    | null   | 0        | root    | 1686248350470 | 1686248350470 | root________ | 1          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| 2  | null | 2    | 1      | 0        | menu    | 1686248350470 | 1686248350470 | menu________ | 1          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| 3  | null | 2    | 1      | 1        | toolbar | 1686248350470 | 1686248350470 | toolbar_____ | 1          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| 4  | null | 2    | 1      | 2        | unfiled | 1686248350470 | 1686248350470 | unfiled_____ | 1          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| 5  | null | 2    | 1      | 3        | mobile  | 1686248350470 | 1686248350470 | mobile______ | 1          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
| 6  | null | 3    | 1      | 0        | null    | 1             | 1             | bookmarkguid | 2          | 1                 |
+----+------+------+--------+----------+---------+---------------+---------------+--------------+------------+-------------------+
test db::schema::tests::test_bookmark_tombstone_auto_created ... FAILED

failures:

---- db::schema::tests::test_bookmark_tombstone_auto_created stdout ----
CHECKING GUID root________
CHECKING GUID menu________
CHECKING GUID toolbar_____
CHECKING GUID unfiled_____
CHECKING GUID mobile______
CHECKING GUID bookmarkguid
```

It's unfortunate that the output of `print_table()` goes to the tty while the output of `dbg` goes to `stderr`, so
you might find the output isn't quite intermingled as you would expect, but it's better than nothing!
