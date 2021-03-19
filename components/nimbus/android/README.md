# The Nimbus SDK, megazorded with Application Services

This is a re-packaging of the [Nimbus SDK](https://github.com/mozilla/nimbus-sdk/) for Android,
compiled in a way that works nicely with the application-services [megazord](../../docs/design/megazords.md).

The Nimbus SDK builds its own standalone `org.mozilla.experiments:nimbus` package, which includes both
the Nimbus Kotlin bindings and the compiled Rust code. However, that package currently isn't published
anywhere, and it's difficult for other Rust components to integrate with Rust code compiled into a
standalone shared library.

Instead, we recommend consumers depend on the `org.mozilla.appservices:nimbus` package, which
provides the same Nimbus Kotlin bindings, but configured to load the underlying Rust code from
the application-serivces `:full-megazord` package. This lets us compile the Rust code for Nimbus
together with the Rust code for Glean and for application-services, and have them interoperate
directly at the Rust level.

Consumers will also need to add the following snippet to ensure that Nimbus can find the
correct shared library:

```
    System.setProperty(
        "uniffi.component.nimbus.libraryOverride",
        System.getProperty("mozilla.appservices.megazord.library", "megazord")
    )
```

(The Nimbus SDK wrapped in android-components handles this for you automatically.


To update Nimbus:

1. Select the release version to which to update, `$version`.
2. Update the submodule to that version (replace `$version` below with the correct version, e.g. `0.3.0`):

    ```
    cd components/external/nimbus-sdk
    git fetch origin
    git checkout v$version
    ```
3. Update `Cargo.lock` to reflect any upstream changes:
    ```
    cargo update
    ```
4. Commit the changes:

    ```
    git add components/external/nimbus-sdk
    git add Cargo.lock
    git commit
    ```
5. Run `./gradlew nimbus:test` to ensure that things still work correctly for Android.
   If this returns an error, you may need to update `./build.gradle` to track
   any build changes made in the upstream Nimbus repository.
6. Run an Xcode build to ensure everything compiles correctly for iOS.
