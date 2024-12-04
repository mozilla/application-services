# High level design for shipping Rust Components as Swift Packages
> This is a high level description of the decision highlighted in the [ADR that introduced Swift Packages as a strategy to ship our Rust components](../adr/0003-swift-packaging.md). That document includes that tradeoffs and why we chose this approach.

<!--
  N.B. you can edit this image in Google Docs and changes will be reflected automatically:

    https://docs.google.com/drawings/d/1tX05I-e6hNBQxch7PescDH7k4G7ddAJwXDPoIqp1RYk/edit
-->
<img src="https://docs.google.com/drawings/d/e/2PACX-1vRnyxy7VjdD3bYTso8V3AL5FpIQ4_S54dOCDI6fxfZEbG3_CVBwZZP1uLYbUVE9M54GSXUkNgewzOQm/pub?w=720&h=540" width="720" height="540" alt="A box diagram describing how the rust-components-swift repo, applicaiton-services repo, and MozillaRustComponents XCFramework interact">

The strategy includes two main parts:
- The [xcframework](https://developer.apple.com/documentation/swift_packages/distributing_binary_frameworks_as_swift_packages) that is built from a [megazord](./megazords.md). The xcframework contains the following, built for all our target iOS platforms.
     - The compiled Rust code for all the crates listed in `Cargo.toml` as a static library
    - The C header files and [Swift module maps](https://clang.llvm.org/docs/Modules.html) for the components
- The [`rust-components-swift`](https://github.com/mozilla/rust-components-swift) repository which has a `Package.swift` that includes the `xcframework` and acts as the swift package the consumers import


## The xcframework and `application-services`
In `application-services`, in the [`megazords/ios-rust`](https://github.com/mozilla/application-services/tree/main/megazords/ios-rust) directory, we have the following:
- A Rust crate that serves as the [megazord](./megazords.md) for our iOS distributions. The megazord depends on all the Rust Component crates and re-exports their public APIs.
- Some skeleton files for building an xcframework:
        1. [`module.modulemap`](https://clang.llvm.org/docs/Modules.html): The module map tells the Swift compiler how to use C APIs.
        1. `MozillaRustComponents.h`: The header is used by the module map as a shortcut to specify all the available header files
        1. `Info.plist`: The `plist` file specifies metadata about the resulting xcframework. For example, architectures and subdirectories.
- The `build-xcframework.sh` script that stitches things together into a full xcframework bundle:
    - The `xcframework` format is not well documented; briefly:
        - The xcframework is a directory containing the resources compiled for multiple target architectures. The xcframework is distributed as a `.zip` file.
        - The top-level directory contains a subdirectory per architecture and an `Info.plist`. The `Info.plist` describes what lives in which directory.
        - Each subdirectory represents an architecture. And contains a `.framework` directory for that architecture.

> It's a little unusual that we're building the xcframework by hand, rather than defining it as the build output of an Xcode project. It turns out to be simpler for our purposes, but does risk diverging from the expected format if Apple changes the details of xcframeworks in future Xcode releases.

## The `rust-components-swift` repository
The repository is a Swift Package for distributing releases of Mozilla's various Rust-based application components. It provides the Swift source code packaged in a format understood by the Swift package manager, and depends on a pre-compiled binary release of the underlying Rust code published from `application-services`

The `rust-components-swift` repo mainly includes the following:
- `Package.swift`: Defines all the [`targets`](https://developer.apple.com/documentation/swift_packages/target) and [`products`](https://developer.apple.com/documentation/swift_packages/product) the package exposes.
    - `Package.swift` also includes where the package gets the `xcframework` that `application-services` builds
- `make_tag.sh`: A script that does the following:
    - Generates any dynamically generated Swift code, mainly:
        - The [uniffi](https://github.com/mozilla/uniffi-rs/) generated Swift bindings
        - [The Glean metrics](https://mozilla.github.io/glean/book/user/adding-glean-to-your-project/swift.html#setting-up-metrics-and-pings-code-generation)
    - Creates and commits a git tag that can be pushed to cut a release

> Consumers would then import the `rust-components-swift` swift package, by indicating the url of the package on github (i.e <https://github.com/mozilla/rust-components-swift>) and selecting a version using the git tag.
