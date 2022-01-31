**See [the release process docs](docs/howtos/cut-a-new-release.md) for the steps to take when cutting a new release.**

# Unreleased Changes

[Full Changelog](https://github.com/mozilla/application-services/compare/v90.0.1...main)

<!-- WARNING: New entries should be added below this comment to ensure the `./automation/prepare-release.py` script works as expected.

Use the template below to make assigning a version number during the release cutting process easier.

## [Component Name]

### ⚠️ Breaking Changes ⚠️
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's Changed
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))
### What's New
  - Description of the change with a link to the pull request ([#0000](https://github.com/mozilla/application-services/pull/0000))

-->

## Nimbus FML

### What's New
  - The Nimbus FML can now generate swift code for the feature manifest. ([#4780](https://github.com/mozilla/application-services/pull/4780))
    - It can be invoked using:
    ```sh
    $ nimbus-fml <FEATURE_MANIFEST_YAML> -o <OUTPUT_NAME> ios features
    ```
    - You can check the support flags and options by running:
    ```sh
    $ nimbus-fml ios --help
    ```
    - The generated code exposes:
      -  a high level nimbus object, whose name is configurable using the `--classname` option. By default the object is `MyNimbus`.
      - All the enums and objects defined in the manifest as idiomatic Swift code.
    - Usage:
      - To access a feature's value:
        ```swift
        // MyNimbus is the class that holds all the features supported by Nimbus
        // MyNimbus has an singleton instance, you can access it using the `shared` field:

        let nimbus = MyNimbus.shared

        // Then you can access the features using:
        // MyNimbus.features.<featureNameCamelCase>.value(), for example:

        let feature = nimbus.features.homepage.value()
        ```
      - To access a field in the feature:
        ```swift
        // feature.<propertyNameCamelCase>, for example:

        assert(feature.sectionsEnabled[HomeScreenSection.topSites] == true)
        ```

### ⚠️ Breaking Changes ⚠️

  - **Android only**: Accessing drawables has changed to give access to the resource identifier.
    - Migration path to the old behaviour is:

    ```kotlin
    let drawable: Drawable = MyNimbus.features.exampleFeature.demoDrawable
    ```

    becomes:
    ```kotlin
    let drawable: Drawable = MyNimbus.features.exampleFeature.demoDrawable.resource
    ```
