# Fatal Sqlite Errors in app-services

Our components often see sqlite errors - we currently ignore (as in, just pass
them as errors back to our caller) them, but there are some we should handle.

In particular, we should try and identify non-recoverable sqlite errors and
delete the problematic database. This isn't actually data-loss - the data-loss
happened when the database become corrupt. So think of this as making the app
usable again!

Note that this strategy doesn't exclude better recovery strategies - eg,
we could do something smart like auto-backup the database and automatically
restore when we are in this state. Sync will also help users recover their
data. But even if recovery strategies are available, we still need to know
how to identify scenarios where this recovery should be done. Further, there's
no need to wait for better recovery strategies - the user has already lost
their bookmarks *and* they are unable to create new ones - deleting the DB
is a better outcome for the user than not deleting it due to the lack of
a backup/restore story.

All that said though, mis-classifying an error as non-recoverable when it's
actually transient *is* data-loss. Therefore we must remain conservative.
In the future we might leverage telemetry to help us in questionable cases
(for example, telemetry might be able to tell us if recovery from a
particular error ever happens in practice).

To get started, let's enumerate and classify some of the errors we see in
sentry.

TODO:

* Classify these errors into ones we are sure are fatal and open bugs to
  handle them.

* Look at the non-fatal ones (eg, ones that probably mean "out of disk space")
  and see if further action should be taken (eg, should we report them to sentry
  in a different way? A special error code we can return to the app so they can
  tell the user disk space is a problem? Anything else more graceful we should do?

## Errors

### RustErrorException: places::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: SystemIOFailure, extended_code: 4874 }, Some("disk I/O error")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9413305/
#### Notes:
Code implies temporary - probably out of space.

### Error executing SQL: unable to open database file: /data/data/org.mozilla.firefox/files/places.sqlite
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9368101
* https://sentry.prod.mozaws.net/operations/fenix/issues/6500299
#### Notes
Fairly useless - no error codes etc. We should work out why.

### places::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: CannotOpen, extended_code: 14 }, Some("unable to open database file: /data/data/org.mozilla.firefox/files/places.sqlite")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9368099/
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9356765

### logins::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: CannotOpen, extended_code: 14 }, Some("unable to open database file: /mnt/expand/d20459bf-e555-4f29-85de-c6aafe669925/user/0/org.mozilla.firefox/databases/logins.sqlite")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9363635/

### PlacesException: Error executing SQL: disk I/O error
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix/issues/8702494
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9376349
* https://sentry.prod.mozaws.net/operations/fenix/issues/6494087
#### Notes
Fairly useless - no error codes etc. We should work out why.

### logins::ffi - Not a database / invalid key error
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9356703
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9363638
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9356681
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9341715
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9359378
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9270379
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9363634
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9268906

### places::ffi - Database busy: Error { code: DatabaseBusy, extended_code: 5 } Some("database is locked")
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9281974 - via places_note_observation
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9375831 - via migration.
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9416290 - via places_delete_everything


### places::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: DatabaseCorrupt, extended_code: 11 }, Some("database disk image is malformed")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9298060
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9360837


### places::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: SystemIOFailure, extended_code: 4874 }, Some("disk I/O error")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9349980
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9349978
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9349979

### places::ffi - Unexpected error: SqlError(SqliteFailure(Error { code: ConstraintViolation, extended_code: 1811 }, Some("FOREIGN KEY constraint failed")))
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9386386/

### Error(Error executing SQL: database or disk is full)
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9414795
#### Notes
This is push - might not even be sqlite - however, there are some like this
for our databases.

## Import failures:

Too late to do much about these, but for completeness...

### places::ffi - Unexpected error: UnsupportedDatabaseVersion(23)
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9362051
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9362050

### logins::ffi - Unexpected error: NonEmptyTable
#### Reports
* https://sentry.prod.mozaws.net/operations/fenix-fennec/issues/9264432
