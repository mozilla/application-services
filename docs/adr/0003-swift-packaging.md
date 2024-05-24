# Distributing Swift Packages

* Status: accepted
* Deciders: rfkelly
* Date: 2021-07-22

## Context and Problem Statement

Our iOS consumers currently obtain application-services as a pre-compiled `.framework` bundle
distributed via [Carthage](https://github.com/Carthage/Carthage). The current setup is not
compatible with building on new M1 Apple Silicon machines and has a number of other problems.
As part of a broader effort to modernize the build process of iOS applications at Mozilla,
we have been asked to re-evaluate how application-services components are dsitributed for iOS.

See [Problems with the current setup](#problems-with-the-current-setup) for more details.

## Decision Drivers

* Ease-of-use for iOS consumers.
* Compatibility with M1 Apple Silicon machines.
* Consistency with other iOS components being developed at Mozilla.
* Ability for the Nimbus Swift bindings to easily depend on Glean.
* Ease of maintainability for application-services developers.

## Considered Options

* **(A) Do Nothing**
  * Keep our current build and distribution setup as-is.
* **(B) Use Carthage to build XCFramework bundles**
  * Make a minimal change to our Carthage setup so that it builds
    the newer XCFramework format, which can support M1 Apple Silicon.
* **(C) Distribute a single pre-compiled Swift Package**
  * Convert the all-in-one `MozillaAppServices` Carthage build to a similar
    all-in-one Swift Package, distributed as a binary artifact.
* **(D) Distribute multiple source-based Swift Package targets, with pre-compiled Rust code**
  * Split the all-in-one `MozillaAppServices` Carthage build into a separate
    Swift Package target for each component, with a shared dependency on pre-compiled
    Rust code as a binary artiact.

## Decision Outcome

Chosen option: **(D) Distribute multiple source-based Swift Packages, with pre-compiled Rust code**.

This option will provide the best long-term consumer experience for iOS developers, and
has the potential to simplify maintenance for application-services developers after an
initial investment of effort.

### Positive Consequences

* Swift packages are very convenient to consume in newer versions of Xcode.
* Different iOS apps can choose to import a different subset of the available components,
  potentiallying helping keep application size down.
* Avoids issues with mis-matched Swift version between application-services build
  and consumers, since Swift files are distributed in source form.
* Encourages better conceptual separation between Swift code for different components;
  e.g. it will make it possible for two Swift components to define an item of the same
  name without conflicts.
* Reduces the need to use Xcode as part of application-services build process, in favour
  of command-line tools.

### Negative Consequences

* More up-front work to move to this new setup.
* We may be less likely to notice if our build setup breaks when used from within Xcode,
  because we're not exercising that code path ourselves.
* May be harder to concurrently publish a Carthage framework for current consumers who aren't
  able to move to Swift packages.
* There is likely to be some amount of API breakage for existing consumers, if only in having
  to replace a single `import MozillaAppServices` with independent imports of each component.

### Implementation Sketch

We will maintain the existing Carthage build infrastructure in the application-services repo and continue publishing a pre-built Carthage framework,
to support firefox-ios until they migrate to Swift Packages.

We will add an additional iOS build task in the application-services repo, that builds *just the Rust code* as a `.xcframework` bundle.
An initial prototype shows that this can be achieved using a relatively straightforward shell script, rather than requiring a second Xcode project.
It will be published as a `.zip` artifact on each release in the same way as the current Carthage framework.
The Rust code will be built as a static library, so that the linking process of the consuming application can pull in
just the subset of the Rust code that is needed for the components it consumes.

We will initially include only Nimbus and its dependencies in the `.xcframework` bundle,
but will eventually expand it to include all Rust components (including Glean, which will continue
to be included in the `application-services` repo as a git submodule)

We will create a new repository `rust-components-swift` to serve as the root of the new Swift Package distribution.
It will import the `application-services` repository as a git submodule. This will let us iterate quickly on the
Swift packaging setup without impacting existing consumers.

We will initially include only Nimbus and its dependencies in this new repository, and the Nimbus swift code
it will depend on Glean via the external `glean-swift` package. In the future we will publish all application-services
components that have a Swift interface through this repository, as well as Glean and any future Rust components.
(That's why the repository is being given a deliberately generic name).

The `rust-components-swift` repo will contain a `Package.swift` file that defines:

* A single binary target that references the pre-built `.xcframework` bundle of Rust code.
* One Swift target for each component, that references the Swift code from the git submodule
  and depends on the pre-built Rust code.

We will add automation to the `rust-components-swift` repo so that it automatically tracks
releases made in the `application-services` repo and creates a corresponding git tag for
the Swift package.

At some future date when all consumers have migrated to using Swift packages, we will remove
the Carthage build setup from the application-services repo.

At some future date, we will consider whether to move the `Package.swift` definition in to the `application-services` repo,
or whether it's better to keep it separate. (Attempting to move it into the `application-services` will involve non-trivial
changes to the release process, because the checksum of the released `.xcframework` bundle needs to be included in
the release tagged version of the `Package.swift` file.)

# Pros and Cons of the Options

### **(A) Do Nothing**

In this option, we would make no changes to our iOS build and publishing process.

* Good, because it's the least amount of work.
* Neutral, because it doesn't change the maintainability of the system for appservices
  developers.
* Neutral, because it doesn't change the amount of separation between Swift code
  for our various components.
* Neutral, because it doesn't address the Swift version incompatibility issues around
  binary artifacts.
* Bad, because it will frustrate consumers who want to develop on M1 Apple Silicon.
* Bad, because it may prevent consumers from migrating to a more modern build setup.
* Bad, because it would prevent consumers from consuming Glean as a Swift package;
  we would require them to use the Glean that is bundled in our build.

This option isn't really tractable for us, but it's included for completeness.

### **(B) Use Carthage to build XCFramework bundles**

In this option, we would try to change our iOS build and publishing process as little
as possible, but use Carthage's recent support for [building platform-independent
XCFrameworks](https://github.com/Carthage/carthage#building-platform-independent-xcframeworks-Xcode-12-and-above) in order
to support consumers running on M1 Apple Silicon.

* Good, because the size of the change is small.
* Good, because we can support development on newer Apple machines.
* Neutral, because it doesn't change the maintainability of the system for appservices
  developers.
* Neutral, because it doesn't change the amount of separation between Swift code
  for our various components.
* Neutral, because it doesn't address the Swift version incompatibility issues around
  binary artifacts.
* Bad, because our iOS consumers have expressed a preference for moving away from Carthage.
* Bad, because other iOS projects at Mozilla are moving to Swift Packages, making
  us inconsistent with perceived best practice.
* Bad, because it would prevent consumers from consuming Glean as a Swift package;
  we would require them to use the Glean that is bundled in our build.
* Bad, because consumers don't get to choose which components they want to use (without
  us building a whole new "megazord" with just the components they want).

Overall, current circumstances feel like a good opportunity to invest a little more
time in order to set ourselves up for better long-term maintainability
and happier consumers. The main benefit of this option (it's quicker!) is less attractive
under those circumstances.

### **(C) Distribute a single pre-compiled Swift Package**

In this option, we would compile the Rust code and Swift code for all our components into
a single `.xcframework` bundle, and then distribute that as a
[binary artifact](https://developer.apple.com/documentation/swift_packages/distributing_binary_frameworks_as_swift_packages) via Swift Package. This is similar to the approach
currently taken by Glean (ref [Bug 1711447](https://bugzilla.mozilla.org/show_bug.cgi?id=1711447))
except that they only have a single component.

* Good, because Swift Packages are the preferred distribution format for new iOS consumers.
* Good, because we can support development on newer Apple machines.
* Good, because it aligns with what other iOS component developers are doing at Mozilla.
* Neutral, because it doesn't change the maintainability of the system for appservices
  developers.
    * (We'd need to keep the current Xcode project broadly intact).
* Neutral, because it doesn't change the amount of separation between Swift code
  for our various components.
* Neutral, because it doesn't address the Swift version incompatibility issues around
  binary artifacts.
* Neutral, because it would prevent consumers from consuming Glean as a separate Swift package;
  they'd have to get it as part of *our* all-in-one Swift package.
* Bad, because it's a larger change and we have to learn about a new package manager.
* Bad, because consumers don't get to choose which components they want to use (without
  building a whole new "megazord" with just the components they want).

Overall, this option would be a marked improvement on the status quo, but leaves out some potential
improvements. For not that much more work, we can make some of the "Neutral" and "Bad" points
here into "Good" points.

### **(D) Distribute multiple source-based Swift Packages, with pre-compiled Rust code**

In this option, we would compile *just the Rust code* for all our components into a single
`.xcframework` bundle and distribute that as a [binary artifact](https://developer.apple.com/documentation/swift_packages/distributing_binary_frameworks_as_swift_packages) via Swift Package.
We would then declare a separate Swift *source* target for the Swift wrapper of each component,
each depending on the compiled Rust code but appearing as a separate item in the Swift package
definition.

* Good, because Swift Packages are the preferred distribution format for new iOS consumers.
* Good, because we can support development on newer Apple machines.
* Good, because it aligns with what other iOS component developers are doing at Mozilla.
* Good, because it can potentially simplify the maintenance of the system for appservices
  developers, by removing Xcode in favour of some command-line scripts.
* Good, because it introduces strict separation between the Swift code for each component,
  instead of compiling them all together in a single shared namespace.
* Good, because the Nimbus Swift package could cleanly depend on the Glean Swift package.
* Good, because consumers can choose which components they want to include.
* Good, because it avoids issues with Swift version incompatibility in binary artifacts.
* Bad, because it's a larger change and we have to learn about a new package manager.

The only downside to this option appears to be the amount of work involved, but an initial
prototype has given us some confidence that the change is tractable and that it may lead
to a system that is easier to maintain over time. It is thus our preferred option.

# Appendix

## Further Reading

* [Bug 1711447](https://bugzilla.mozilla.org/show_bug.cgi?id=1711447) has good historical context on the work to move Glean to using a Swift Package.
* Some material on swift packages:
  * [Managing dependencies using the Swift Package Manager](https://www.swiftbysundell.com/articles/managing-dependencies-using-the-swift-package-manager/) was a useful overview.
  * [Understanding Swift Packages and Dependency Declarations](https://www.timc.dev/posts/understanding-swift-packages/) gives a bit of a deeper dive into having multiple targets
  with different names in a single package.
* Outputs of initial prototype:
  * A prototype of Option (C): [Nimbus + Glean as a pre-built XCFramework Swift Package](https://github.com/mozilla/application-services/pull/4216)
  * A prototype of Option (D): [Rust code as XCFRamework](https://github.com/mozilla/application-services/pull/4225) plus a [Multi-product Swift Package](https://github.com/rfk/rust-components-swift) that depends on it.
      * A [video demo](https://drive.google.com/file/d/12xsOMoFkxHZAEZ8tL5gBaiTYTYoQN1OO/view) of the resulting consumer experience.

## Problems with the current setup

It doesn't build for M1 Apple Silicon machines, because it's not possible to support
both arm64 device builds and arm64 simulator builds in a single binary `.framework`.

Carthage is dispreferred by our current iOS consumers.

We don't have much experience with the setup on the current Application Services team,
and many of its details are under-documented. Changing the build setup requires Xcode
and some baseline knowledge of how to use it.

All components are built as a single Swift module, meaning they can see each other's
internal symbols and may accidentally conflict when naming things. For example we can't
currently have two components that define a structure of the same name.

Consumers can only use the pre-built binary artifacts if they are using the same
version of Xcode as was used during the application-services build. We are not able
to use Swift's `BUILD_LIBRARY_FOR_DISTRIBUTION` flag to overcome this, because some
of our dependencies do not support this flag (specifically, the Swift protobuf lib).
