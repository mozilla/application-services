The iOS 'megazord' builds all the components into a single library. This is built as a static framework.

### Adding new components

- Update `base.xcconfig` HEADER_SEARCH_PATHS to search for headers in the added component
- Add any C bridging headers to the includes in  `MozillaAppServices.h`.
- drag and drop all the swift files from the new component into this project
- update `rust/Cargo.toml` and `rust/src/lib.rs` with the new ffi path

### Glean component

At the moment the MozillaAppServices iOS framework bundles [Glean].
It depends on the releases `glean-ffi` crate from [crates.io] and the corresponding iOS source code.
Glean is bundled as a [git submodule] in `components/glean` and the Xcode project references those files.

To update Glean:

1. Update the version of `glean-ffi` in `megazord/ios/rust/Cargo.toml`
2. Ensure `cargo` updates the versions by running `cargo fetch`
3. Update the submodule to the same version (replace `$version` below with the correct version, e.g. `32.3.0`):

   ```
   cd components/glean
   git fetch origin
   git checkout v$version
   ```
4. Commit the changes:

    ```
    git add components/glean
    git add megazord/ios/rust/Cargo.toml
    git add Cargo.lock
    ```
5. Run an Xcode build to ensure everything compiles.

[Glean]: https://github.com/mozilla/glean
[crates.io]: https://crates.io/crates/glean-ffi
[git submodule]: https://git-scm.com/docs/git-submodule
