# Using locally-published components in Firefox for iOS

It's often important to test work-in-progress changes to this repo against a real-world
consumer project. Here are our current best-practices for approaching this on iOS:

1. Make a local build of the application-services framework using `./build-carthage.sh`.
1. Checkout and `carthage bootstrap` the consuming app (for example using [these instructions with Firefox for
   iOS](https://github.com/mozilla-mobile/firefox-ios#building-the-code)).
1. In the consuming app, replace the application-services framework with a copy of your local build. For example:

   ```
   rm -rf Carthage/Build/iOS/MozillaAppServices.framework
   rsync -ad path/to/application-services/Carthage/Build/iOS/MozillaAppServices.framework/ Carthage/Build/iOS/MozillaAppServices.framework/
   ```
1. Open the consuming app project in XCode and build it from there.

After making changes to application-services code, you will need to re-run these steps in order to
copy the latest changes over into the consuming app.

Firefox for iOS also has a helper script that automates these steps:
[`appservices_local_dev.sh`](https://github.com/mozilla-mobile/firefox-ios/blob/main/appservices_local_dev.sh).

Note that for firefox-ios specifically, you may also need to copy the Glean `sdk_generator.sh` script
from the appservices build into the root of the firefox-ios repository.
