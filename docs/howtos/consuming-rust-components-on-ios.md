# Guide to Consuming Rust Components on iOS

The application services libraries are published as a single zip file containing all the individual component frameworks (such as *Logins.framework*, *FxAClient.framework*) and also a single composite (megazord) framework called *MozillaAppServices.framework* containing all the components.

The client-side can choose to use a single component framework, or the composite.

The package is published as a release on github: https://github.com/mozilla/application-services/releases

## Carthage

- Add the dependency line to the Cartfile, for instance: `github "mozilla/application-services" ~> "v0.16.1"` 
- `carthage` will download MozillaAppServices.frameworks.zip, and add all the available frameworks to the 'Carthage/' dir.
- Choose which framework to link against for your project (in the *Link Binary with Libraries* step in your Xcode target).
- Add additional dependencies, see below.

## Additional dependencies

The project has additional 3rd-party dependencies that a client must link against.

### Protobuf

- Add to the Cartfile: `github "apple/swift-protobuf" ~> 1.0`
- add *SwiftProtoBuf.framework* to  *Link binary with Libraries* for the Xcode target.





