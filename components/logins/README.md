# Logins

Logins implements encrypted storage for login records on top of SQLcipher, with
support for Sync (using the sync15 crate). It uses a slight modification on top
of the database schema that firefox-ios used, however many of the queries and
the way the sync is performed are different, as to allow syncs to complete with
fewer database operations. See the header comment in `src/schema.rs` for an
overview of the schema.

The relevant directories are as follows:

- `src`: The meat of the library. This contains cross-platform rust code that
  implements the actual storage and sync of login records.
- `example`: This contains example rust code for syncing, displaying, and
  editing logins using the code in `src`.
- `ffi`: The Rust public FFI bindings. This is a (memory-unsafe, by necessity)
  API that is exposed to Kotlin and Swift. It leverages the `ffi_support` crate
  to avoid many issues and make it more safe than it othrwise would be. At the
  time of this writing, it uses JSON for marshalling data over the FFI, however
  in the future we will likely use protocol buffers.
- `android`: This contains android bindings to logins, written in Kotlin. These
  use JNA to call into to the code in `ffi`.
- `ios`: This contains the iOS binding to logins, written in Swift. These use
  Swift's native support for calling code written in C to call into the code in
  `ffi`.
