# Sqlite Database Pragma Usage

The data below has been added as a tool for future pragma analysis work and is expected to be useful so long as our pragma usage remains stable or this doc is kept up-to-date. This should help us understand our current pragma usage and where we may be able to make improvements.

| Pragma                                                                             | Value   | Component                                      | Notes |
|------------------------------------------------------------------------------------|---------|------------------------------------------------|-------|
| [cache_size](https://www.sqlite.org/pragma.html#pragma_cache_size)                 | -6144   | places                                         |       |
| [foreign_keys](https://www.sqlite.org/pragma.html#pragma_foreign_keys)             | ON      | autofill, places, tabs, webext-storage         |       |
| [journal_mode](https://www.sqlite.org/pragma.html#pragma_journal_mode)             | WAL     | autofill, places, tabs, webext-storage         |       |
| [page_size](https://www.sqlite.org/pragma.html#pragma_page_size)                   | 32768   | places                                         |       |
| [secure_delete](https://www.sqlite.org/pragma.html#pragma_secure_delete)           | true    | logins                                         |       |
| [temp_store](https://www.sqlite.org/pragma.html#pragma_temp_store)                 | 2       | autofill, logins, places, tabs, webext_storage | Setting `temp_store` to 2 (MEMORY) is necessary to avoid [SQLITE_IOERR_GETTEMPPATH](https://www.javadoc.io/doc/org.xerial/sqlite-jdbc/3.15.1/org/sqlite/SQLiteErrorCode.html#SQLITE_IOERR_GETTEMPPATH) errors on Android (see [here](https://github.com/mozilla/application-services/blob/62bef6a0d49f8ab17b969e8ce482ac12c53f0987/components/logins/src/db.rs#L79) for details) |
| [wal_autocheckpoint](https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint) | 62      | places                                         |       |
| [wal_checkpoint](https://www.sqlite.org/pragma.html#pragma_wal_checkpoint)         | PASSIVE | places                                         | Used in the `sync finished` step in history and bookmarks syncing and in the places `run_maintenance` function |

- The [user_version](https://www.sqlite.org/pragma.html#pragma_user_version) pragma is excluded because the value varies and sqlite does not do anything with the value.
- The push component does not implement any of the commonly used pragmas noted above.
- [The sqlcipher pragmas](https://www.zetetic.net/sqlcipher/sqlcipher-api/) that we set have been excluded from this list as we are trying to remove sqlcipher and do not want to encourage future use.
