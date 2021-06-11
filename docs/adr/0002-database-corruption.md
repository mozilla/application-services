# Handling Database Corruption

* Status: accepted
* Date: 2021-06-08

## Context and Problem Statement

Some of our users have corrupt SQLite databases and this makes the related
component unusable.  The best way to deal with corrupt databases is to simply
delete the database and start fresh (#2628).  However, we only want to do this
for persistent errors, not transient errors like programming logic errors, disk
full, etc.  This ADR deals with 2 related questions:

  * A) When and how do we identify corrupted databases?
  * B) What do we do when we identify corrupted databases?

## Decision Drivers

* Deleting valid user data should be avoided at almost any cost
* Keeping a corrupted database around is almost as bad.  It currently prevents
  the component from working at all.
* We don't currently have a good way to distinguish between persistent and
  transient errors, but this can be improved by reviewing telemetry and sentry
  data.

## Considered Options

* A) When and how do we identify corrupted databases?
  * 1: Assume all errors when opening a database are from corrupt databases
  * 2: Check for errors when opening a database and compare against known corruption error types
  * 3: Check for errors for all database operations and compare against known corruption error types
* B) What do we do when we identify corrupted databases?
  * 1: Delete the database file and recreate the database
  * 2: Move the database file and recreate the database
  * 3: Have the component fail

## Decision Outcome

* A2: Check for errors when opening a database and compare against known corruption error types
* B1: Delete the database file and recreate the database

Decision B follows from the choice of A.  Since we're being conservative in
identifying errors, we can delete the database file with relative confidence.

"Check for errors for all database operations and compare against known
corruption error types" also seems like a reasonable solution that we may
pursue in the future, but we decided to wait for now.  Checking for errors
during opening time is the simpler solution to implement and should fix the
issue in many cases.  The plan is to implement that first, then monitor
sentry/telemetry to decide what to do next.

# Pros and Cons of the Options

### A1: Assume all errors when opening a database are from corrupt databases
* Good, because the sentry data indicates that many errors happen during opening time
* Good, because migrations are especially likely to trigger corruption errors
* Good, because it's a natural time to delete the database -- the consumer code
  hasn't run any queries yet and doesn't have any open connections.
* Bad, because it will delete valid user data in several situations that are
  relatively common: migration logic errors, OOM errors, Disk full.

### A2: Check for errors when opening a database and compare against known corruption error types (Decided)
* Good, because should eliminate the possibility of deleting valid user data.
* Good, because the sentry data indicates that many errors happen during opening time
* Good, because it's a natural time to delete the database -- the consumer code
  hasn't run any queries yet and doesn't have any open connections.
* Bad, because we don't currently have a good list corruption errors

### A3: Check for errors for all database operations and compare against known corruption error types
* Good, because the sentry data indicates that many errors happen outside of opening time
* Good, because should eliminate the possibility of deleting valid user data.
* Bad, because the consumer code probably doesn't expect the database to be
  deleted and recreated in the middle of a query.  However, this is just an
  extreme case of normal database behavior -- for example any given row can be
  deleted during a sync.
* Bad, because we don't currently have a good list corruption errors

### B1: Delete the database file and recreate the database (Decided)
* Good, because it would allow users with corrupted databases to use the
  affected components again
* Bad, because any misidentification will lead to data loss.

### B2: Move the database file and recreate the database

This option would be similar to 1, but instead of deleting the file we would
move it to a backup location.  When we started up, we could look for backup
files and try to import lost data.

* Good, because if we misidentify corrupt databases, then we have the
  possibility of recovering the data
* Good, because it allows a way for users to delete their data (in theory).
  If the consumer code executed a `wipe()` on the database, we could also
  delete any backup data.
* Bad, because it's very difficult to write a recovery function that merged
  deleted data with any new data.  This function would be fairly hard to test
  and it would be easy to introduce a new logic error.
* Bad, because it adds significant complexity to the database opening code
* Bad, because the user experience would be strange.  A user would open the
  app, discover that their data was gone, then sometime later discover that the
  data is back again.

### B3: Return a failure code

* Good, because this option leaves no chance of user data being deleted
* Good, because it's the simplest to implement
* Bad, because the component will not be usable if the database is corrupt
* Bad, because the user's data is potentially exposed in the corrupted database
  file and we don't provide any way for them to delete it.
