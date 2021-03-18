The iOS 'megazord' builds all the components into a single library.
This is published as a `.framework` containing a dynamic library.

### Adding new components

- Update `base.xcconfig` HEADER_SEARCH_PATHS to search for headers in the added component
- Add any C bridging headers to the includes in  `MozillaAppServices.h`.
- drag and drop all the swift files from the new component into this project
- update `rust/Cargo.toml` and `rust/src/lib.rs` with the new ffi path

### Glean component

At the moment the MozillaAppServices iOS framework bundles [Glean].
Glean is bundled as a [git submodule] in `components/external/glean` and the Xcode project references those files.

To update Glean:

1. Select the release version to which to update, `$version`.
2. Update the submodule to that version (replace `$version` below with the correct version, e.g. `32.3.0`):

    ```
    cd components/external/glean
    git fetch origin
    git checkout v$version
    ```
3. Update `Cargo.lock` to reflect any upstream changes:
    ```
    cargo update -p glean-ffi
    ```
4. Update the dependency summary:
    ```
    tools/regenerate_dependency_summaries.sh
    ```
5. Commit the changes:

    ```
    git add components/external/glean
    git add Cargo.lock
    git commit
    ```
6. Run an Xcode build to ensure everything compiles.

[Glean]: https://github.com/mozilla/glean
[git submodule]: https://git-scm.com/docs/git-submodule
