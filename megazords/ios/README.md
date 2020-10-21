The iOS 'megazord' builds all the components into a single library. This is built as a static framework.

### Adding new components

- Update `base.xcconfig` HEADER_SEARCH_PATHS to search for headers in the added component
- Add any C bridging headers to the includes in  `MozillaAppServices.h`.
- drag and drop all the swift files from the new component into this project
- update `rust/Cargo.toml` and `rust/src/lib.rs` with the new ffi path

### Glean component

At the moment the MozillaAppServices iOS framework bundles [Glean].
Glean is bundled as a [git submodule] in `components/external/glean` and the Xcode project references those files.

To update Glean, see follow the instructions in `components/glean/README.md`,
then run an Xcode build to ensure everything compiles.

[Glean]: https://github.com/mozilla/glean
[git submodule]: https://git-scm.com/docs/git-submodule
