# XCFramework build for distributing Rust code on iOS

This directory contains the logic for compiling all of our Rust code into a binary
artifact that can be easily distributed to iOS consumers. If you run the following
script:

```
$> ./build-xcframework.sh
```

Then it should produce a file named `MozillaRustComponents.xcframework.zip` that
contains:

* The compiled Rust code for all the crates listed in `Cargo.toml`, as a static library,
* along with their corresponding C header files and Swift module maps,
* built for all our target iOS platforms, as an "XCFramework" bundle.

The resulting `.zip` is suitable for consumption as a Swift Package binary dependency.

## What's here?

In this directory we have:

* A Rust crate that serves as the "megazord" for our iOS distributions; it basically depends
  on all the Rust Component crates and re-exports their public APIs.
* Some skeleton files for building an XCFramework:
    * `module.modulemap` is a "module map", which tells the Swift compiler how to use C-level APIs.
    * `MozillaRustComponents.h` is an "umbrella header", used by the module map as a shortcut
      to specify all the available header files.
    * `Info.plist` specified metadata about the resulting XCFramework, such as the available
      architectures and their subdirectories.
* The `build-xcframework.sh` script which knows how to stitch things together into a full
  XCFramework bundle.
    * The XCFramework format is not well documented; briefly:
        * It's a directory containing resources compiled for multiple target architectures,
          typically distributed as `.zip` file.
        * The top-level directory contains a subdirectory per architecture, and an `Info.plist`
          file that says what things live in which directory.
        * Each subdirectory contains a `.framework` directory for that architecture. There
          are notes on the layout of an individual `.framework` in the links below.

It's a little unusual that we're building the XCFramework by hand, rather than defining it
as the build output of an Xcode project. It turns out to be simpler for our purposes, but
does risk diverging from the expected format if Apple changes the detailts of XCFrameworks
in future Xcode releases.

## Adding crates

To add a new crate to the distribution:

1. Update its `uniffi.toml`, if any, to include the following settings:
    ```
    [bindings.swift]
    ffi_module_name = "MozillaRustComponents"
    ffi_module_filename = "<crate_name>FFI"
    ```
1. Add it as a dependency in `Cargo.toml`.
1. Add a `pub use` declaration for it in this `./src/lib.rs`.
1. Add logic to `build-xcframework.sh` to copy or generate its header file into the build.
1. Add a `#import` for its header file to `MozillaRustComponents.h`

## Testing locally

For testing out local changes in a consuming app, you can:

* Take a local checkout of https://github.com/mozilla/rust-components-swift.
> Note: `rust-components-swift` contains a submodule of `application-services`, if you're testing any code changes you'll need
> to push/publish your branch and update the submodule in that repo.
* Run `./build-xcframework.sh` to build the XCFramework bundle.
* Unzip the resulting bundle inside the root of the `rust-components-swift` checkout, and `git add` it.
* Edit `rust-components-swift/Package.swift` and follow the comments on the `MozillaRustComponents`
  binary target to point it at the local path.
* Commit the result to your local checkout, and make a git tag in `MAJOR.MINOR.PATCH` format.
* Add your local `rust-components-swift` repo as a Swift Package dependency in a consuming app,
  by specifying `file:///path/to/rust-components-swift` as the git repo.
    * (You'll have to remove any existing depdency on https://github.com/mozilla/rust-components-swift first)

Note that the XCFramework *must* be committed to the local repo and your changes *must* be included
in a local git tag. Swift Package Manager is very opinionated and will only pull dependencies from a
git tag that it can parse as a semver version number.

## Testing from a pre-release commit

For release builds, we publish the resulting `MozillaRustComponents.xcframework.zip` as a GitHub
release artifact, and then update https://github.com/mozilla/rust-components-swift to point to
it via URL and hash.

For testing from a PR or unreleased git commit, you can:

* Find the CircleCI job named `ios-artifacts` for the commit you want to test, click through to view it on CircleCI,
and confirm that it completed successfully.
* In the "artifacts" list, locate `MozillaRustComponents.xcframework.zip` and note its URL.
* In the "steps" list, find the step named `XCFramework bundle checksum`, and note the checksum in its output.
* Take a local checkout of https://github.com/mozilla/rust-components-swift,
and edit its `Swift.package` to use the above URL and checksum for the `MozillaRustComponents` binary target.
* Commit the result to your local checkout, and make a git tag in `MAJOR.MINOR.PATCH` format.
* Add your local `rust-components-swift` repo as a Swift Package dependency in a consuming app,
  by specifying `file:///path/to/rust-components-swift` as the git repo.
    * (You'll have to remove any existing depdency on https://github.com/mozilla/rust-components-swift first)

Note that your changes *must* be included in a local git tag. Swift Package Manager is very
opinionated and will only pull dependencies from a git tag that it can parse as a semver
version number.

## Further Reading

* The Architecture Design Doc wherein we decided to distribute things this way:
    * [0003-swift-packaging.md](../../docs/adr/0003-swift-packaging.md)
* An introduction to the problem that XCFrameworks as designed to solve:
    * https://blog.embrace.io/xcode-12-and-xcframework/
* A brief primer on the contents of a Framework, which is useful when you want
  to construct one by hand:
    * https://bignerdranch.com/blog/it-looks-like-youre-still-trying-to-use-a-framework/
* The documentation on Module Maps, which is how C-level code gets exposed to Swift:
    * https://clang.llvm.org/docs/Modules.html#module-maps
