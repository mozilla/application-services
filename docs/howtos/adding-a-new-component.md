# Adding a new component to Application Services

This is a rapid-fire list for adding a component from scratch and generating Kotlin/Swift bindings.

## The Rust Code

Your component should live under `./components` in this repo.
Use `cargo new --lib ./components/<your_crate_name>`to create a new library crate.

See the [Guide to Building a Rust Component](./building-a-rust-component.md) for general
advice on designing and structuring the actual Rust code, and follow the
[Dependency Management Guidelines](../dependency-management.md) if your crate
introduces any new dependencies.

Use [UniFFI](https://mozilla.github.io/uniffi-rs/) to define how your crate's
API will get exposed to foreign-language bindings. Lookup the installed uniffi
version on other packages (eg `grep uniffi
components/init_rust_components/Cargo.toml`) and use the same version. Place
the following in your `Cargo.toml`:

```
[dependencies]
uniffi = { version = "<current uniffi version>" }
```

New components should prefer using the
[proc-macro](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html) approach rather than
a UDL file based approach.  If you do use a UDL file, add this to `Cargo.toml` as well.

```
[build-dependencies]
uniffi = { version = "<current uniffi version>" }
```

Include your new crate in the `application-services` workspace, by adding
it to the `members` and `default-members` lists in the `Cargo.toml` at
the root of the repository.

Run `cargo check -p <your_crate_name>` in the repository root to confirm that
things are configured properly. This will also have the side-effect of updating
`Cargo.lock` to contain your new crate and its dependencies.


## The Android Bindings

Run the `cargo start-bindings android <your_crate_name> <component_description>` command to auto-generate the initial code.  Follow the directions in the output.

You will end up with a directory structure something like this:

* `components/<your_crate_name>/`
    * `Cargo.toml`
    * `uniffi.toml`
    * `src/`
        * Rust code here.
    * `android/`
        * `build.gradle`
        * `src/`
          * `main/`
              * `AndroidManifest.xml`

### Dependent crates

If your crate uses types from another crate in it's public API, you need to include a dependency for
the corresponding project in your `android/build.gradle` file.

For example, suppose use the `remote_settings::RemoteSettingsServer` type in your public API so that
consumers can select which server they want.  In that case, you need to a dependency on the
remotesettings project:

```
dependencies {
    api project(":remotesettings")
}
```

### Async dependencies

If your components exports async functions, add the following to your `android/build.gradle` file:

```
dependencies {
    implementation libs.kotlin.coroutines
}
```

### Hand-written code

You can include hand-written Kotlin code alongside the automatically
generated bindings, by placing `.kt`` files in a directory named:
* `./android/src/test/java/mozilla/appservices/<your_crate_name>/`

You can write Kotlin-level tests that consume your component's API,
by placing `.kt`` files in a directory named:
* `./android/src/test/java/mozilla/appservices/<your_crate_name>/`.

You can run the tests with `./gradlew <your_crate_name>:test`

## The iOS Bindings

* Run the `cargo start-bindings ios <your_crate_name>` command to auto-generate the initial code
* Run `cargo start-bindings ios-focus <your_crate_name>` if you also want to expose your component to Focus.
* Follow the directions in the output.


You will end up with a directory structure something like this:

* `components/<your_crate_name>/`
    * `Cargo.toml`
    * `uniffi.toml`
    * `src/`
        * Rust code here.

### Adding your component to the Swift Package Megazord

> *For more information on our how we ship components using the Swift Package Manager, check the [ADR that introduced the Swift Package Manager](../adr/0003-swift-packaging.md)*

Add your component into the iOS ["megazord"](../design/megazords.md) through the local Swift Package Manager (SPM) package `MozillaRustComponentsWrapper`. Note this SPM is for ease of testing APIs locally. The official SPM that is consumed by firefox-ios is a [local package in their repo](https://github.com/mozilla-mobile/firefox-ios/tree/main/MozillaRustComponents).


1. Place your Swift test code in:
   ```
   megazords/ios-rust/tests/MozillaRustComponentsWrapper/
   ```

> Note: swift-specific tests are ideally suited in the consuming app as there is better integration coverage and ensures we're accurately testing how it's being consumed

That's it! At this point, if you don't intend on writing tests _(are you sure?)_ you can skip this next section.

### Writing and Running Tests

The current system combines all rust crates into one binary (megazord). To use your rust APIs simply
import the local SPM into your tests:

```swift
@testable import MozillaRustComponentsWrapper
```

To test your component:

- Run the script:

```
./automation/build_ios_artifacts.sh
```

The script will:
1. Build the XCFramework (combines all rust binaries for SPM)
2. Generate UniFFi bindings (generated files will be found in `megazords/ios-rust/sources/MozillaRustComponentsWrapper/Generated/`)
3. Generate Glean metrics
4. Run any tests found in the test dir mentioned above


### Distribution of the component

The UniFFi files will be generated & packaged via `taskcluster/scripts/build-and-test-swift.py`.

Consumers (eg: Firefox iOS) pull the latest artifacts via a nightly [Gitub action](https://github.com/mozilla-mobile/firefox-ios/actions/workflows/update-appservices-nightly.yml). Once your changes are pulled via a nightly release you'll be able to use your new APIs!

> Note: If you don't want to wait for a nightly, once the CI finishes your build -- you can request the action run in firefox-ios to get it sooner

> Note pt2: If you want to locally test against firefox-ios, follow [this guide](https://github.com/mozilla/application-services/blob/main/docs/howtos/locally-published-components-in-firefox-ios.md)