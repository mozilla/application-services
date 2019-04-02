# Logins

Logins are consumed by Firefox browsers, and the standalone Lockbox app.

Login storage is based on the original [Firefox for iOS implementation](https://github.com/mozilla-mobile/firefox-ios/blob/faa6a2839abf4da2c54ff1b3291174b50b31ab2c/Storage/SQL/SQLiteLogins.swift).

## Architecture

Logins can optionally use SQLCipher for persistence, allowing them to be encrypted on disk.

## Syncing and merging

Login sync uses three-way merges to resolve conflicts.
