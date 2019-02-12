FRAMEWORK_NAME="${FRAMEWORK_NAME:-MozillaAppServices-frameworks.zip}"
## When https://github.com/Carthage/Carthage/issues/2623 is fixed, 
## carthage build --archive should work to produce a zip 
carthage build --no-skip-current --verbose && zip -r $FRAMEWORK_NAME Carthage/Build/iOS

