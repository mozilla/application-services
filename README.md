## Firefox Application Services

Application Services (a-s) is collection of Rust Components that are used to enable Firefox applications to integrate with Firefox accounts, sync, experimentation, etc. Each component is built using a core of shared code written in Rust, wrapped with native language bindings for different platforms.

### Contributing
To contribute, please review the Mozilla [Community Participation Guidelines](https://www.mozilla.org/en-US/about/governance/policies/participation/) and then visit our [how to contribute](docs/contributing.md) guide.

### Contact
Get in touch with other community members on Matrix, the mailing list or through issues here on GitHub.
- Matrix: [#rust-components:mozilla.org](https://chat.mozilla.org/#/room/#rust-components:mozilla.org) ([How to connect](https://wiki.mozilla.org/Matrix#Connect_to_Matrix))
- Mailing list: application-services@mozilla.com

### Building the Rust Components
1. Clone or Download the repository:
```shell
  $ git clone https://github.com/mozilla/application-services (or use the ssh link)
  $ cd application-services
  $ git submodule init
  $ git submodule update --recursive
  ```
1. Follow these instructions to install your [system-level dependencies](docs/building.md#building-application-services)
1. Run the a-s Rust unit tests
```shell
cargo test
```

### Consumer build, integration and testing
The application-services library primary consumers are Fenix (Firefox on Android) and Firefox iOS. Assure you are able to run integration tests (for Android and iOS if using MacOS) by following the instructions to build for Android and iOS integrations.  

#### Android integration builds and helpful tools
* Build instructions to test [Fenix / android-components integration](docs/building.md#building-for-fenix)
* [Fenix Auto-publication workflow for android-components and application-services](https://github.com/mozilla-mobile/fenix/#auto-publication-workflow-for-android-components-and-application-services)


#### Firefox for iOS integration
* Build instructions to test [Firefox iOS integration](docs/building.md#building-for-firefox-ios)
[Using local components in iOS](docs/howtos/locally-published-components-in-ios.md)

#### Firefox Desktop
* Build instructions to test [Firefox Desktop integration](docs/building.md#building-for-firefox-desktop)

### Documentation
We use rustdoc to document both the public API of the components and the various internal implementation details. Once you have completed the build steps, you can view the docs by running:

```shell
cargo doc --no-deps --document-private-items --open
```

The [./docs/](docs) directory holds internal documentation about working with the
code in this repository

### Rust Components

[./components/](components) contains the source for each component, and its
  FFI bindings.
* See [./components/logins/](components/logins) for an example, where you can
    find:
  * The shared [rust code](components/logins/src).
  * The mapping into a [C FFI](components/logins/ffi).
  * The [Kotlin bindings](components/logins/android) for use by Android
      applications.
  * The [Swift bindings](components/logins/ios) for use by iOS applications.
* See [./components/fxa-client](components/fxa-client) for an example that uses
    [uniffi](https://github.com/mozilla/uniffi-rs/) to generate API wrappers for
    multiple languages, such as Kotlin and Swift.

#### List of components
* [autofill](components/autofill) - for storage and syncing of credit card and
  address information
* [crashtest](components/crashtest) - testing-purposes (crashing the Rust code)
* [fxa-client](components/fxa-client) - for applications that need to sign in
  with FxA, access encryption keys for sync, and more.
* [logins](components/logins) - for storage and syncing of a user's saved login
  credentials
* [nimbus](components/nimbus) - for integrating with Mozilla's [experimentation](https://mozilla.github.io/experimenter-docs/) platform for Firefox
* [places](components/places) - for storage and syncing of a user's saved
  browsing history
* [push](components/push) - for applications to receive real-time updates via
  WebPush
* [rc_log](components/rc_log) - for connecting component log output to the
  application's log stream
* [support](components/support) - low-level utility libraries
  * [support/ffi](components/support/ffi) - utilities for building a component's
    FFI bindings
  * [support/rc_crypto](components/rc_crypto) - handles cryptographic needs backed by Mozilla's
    [NSS](https://developer.mozilla.org/en-US/docs/Mozilla/Projects/NSS) library
  * [support/sql](components/support/sql) - utilities for storing data locally
    with SQL
* [sync15](components/sync15) - shared library for accessing data in Firefox
  Sync
* [sync_manager](components/sync_manager) - integrates multiple sync engines/
  stores into a single framework
* [tabs](components/tabs) - an in-memory syncing engine for remote browser tabs
* [viaduct](components/viaduct) - an HTTP request library
* [webext-storage](components/webext-storage) - powers an implementation of the
chrome.storage.sync WebExtension API
