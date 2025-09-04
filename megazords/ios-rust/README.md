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

To support [`focus-ios`](https://github.com/mozilla-mobile/focus-ios) which only needs a subset of the Rust code, we also support generating a smaller xcframework using:

```
$> ./build-xcframework.sh --focus
```

Then it should produce a file named `FocusRustComponents.xcframework.zip` in the `focus` directory that serves as the binary that `focus-ios` eventually consumes.

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
* The `focus` directory, which is a megazord that gets built for `focus-ios`. The components in the `focus` megazord are a subset of the components in the overall `ios-rust` megazord and thus are only built on release.

It's a little unusual that we're building the XCFramework by hand, rather than defining it
as the build output of an Xcode project. It turns out to be simpler for our purposes, but
does risk diverging from the expected format if Apple changes the detailts of XCFrameworks
in future Xcode releases.

## Adding crates

For details on adding new crates, [checkout the documentation for adding new spm components](../../docs/howtos/adding-a-new-component.md#distribute-your-component-with-rust-components-swift)


## Testing local Rust changes
For testing changes against our internal test suites:

> If you've made rust changes:
Run `./automation/build_ios_artifacts.sh`
   - This will generate the XCFramework, which makes the rust binaries, generates the UniFFi bindings, and generates any Glean metrics

Then you'll follow one of the sections below for testing against an actual consumer


## Testing local changes for consumers

See the following documents for testing local changes in consumers:
1. [Testing against firefox-ios](../../docs/howtos/locally-published-components-in-firefox-ios.md)
1. [Testing against focus-ios](../../docs/howtos/locally-published-components-in-focus-ios.md)

## Testing from a pre-release commit

For release builds, we publish the resulting `MozillaRustComponents.xcframework.zip` as a GitHub
release artifact, and then consumers can consume the zip and add it as a dependency via Package.swift. See [firefox-ios](https://github.com/mozilla-mobile/firefox-ios/tree/main/MozillaRustComponents) for an example of how they consume our xcframework.

For testing from a PR or unreleased git commit, you can:

* Find the CircleCI job named `ios-artifacts` for the commit you want to test, click through to view it on CircleCI,
and confirm that it completed successfully.
* In the "artifacts" list, locate `MozillaRustComponents.xcframework.zip` and note its URL.
* In the "steps" list, find the step named `XCFramework bundle checksum`, and note the checksum in its output.
* Update the values in the consuming package in firefox-ios https://github.com/mozilla-mobile/firefox-ios/blob/main/MozillaRustComponents/Package.swift#L4-L6
* In firefox-ios, reset package cache and build!

> Note: You can also just comment out the url version above and have it point to a local xcframework https://github.com/mozilla-mobile/firefox-ios/blob/dc9248398609a77c89e5215a58c5975eef937ac4/MozillaRustComponents/Package.swift#L44-L47

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
