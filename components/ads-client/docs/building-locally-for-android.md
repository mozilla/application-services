# Building Ads-Client Locally for Android Testing

This guide covers how to build the ads-client component locally and test it with Firefox for Android (Fenix).

## Prerequisites

Before building ads-client for Android, ensure you have completed the initial Application Services setup:

1. Follow the [main building instructions](../../../docs/building.md) to set up your development environment
2. Verify you have the required tools installed (Rust, NDK, gyp, ninja, etc.)
3. Ensure you can successfully run `cargo test` in the application-services root

## Initial Setup

### 1. Install Required NDK Version

The ads-client component requires NDK version 29.0.14206865. (Keep updated with docs linked above) Install it using one of these methods:

**Via Android Studio:**
1. Open Android Studio
2. Navigate to Settings/Preferences → Appearance & Behavior → System Settings → Android SDK
3. Click the "SDK Tools" tab
4. Check "Show Package Details"
5. Expand "NDK (Side by side)"
6. Check version **29.0.14206865**
7. Click Apply

**Via command line:**
```bash
$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager --install "ndk;29.0.14206865"
```

### 2. Configure local.properties

Create or edit `local.properties` in the application-services root directory:

```properties
sdk.dir=/path/to/Android/sdk
ndk.dir=/path/to/Android/sdk/ndk/29.0.14206865

# Optimize build speed by targeting specific architectures
# For Apple Silicon Mac with ARM64 emulator:
rust.targets=darwin-aarch64,arm64

# For Intel Mac with x86 emulator:
# rust.targets=darwin-x86-64,x86
```

Replace `/path/to/Android/sdk` with your actual Android SDK path (typically `~/Library/Android/sdk` on macOS or `~/Android/Sdk` on Linux).

### 3. Build NSS Libraries

NSS libraries must be built before compiling the ads-client component:

```bash
cd libs
./build-all.sh android
cd ..
```

Note: This script must be run from within the `libs/` directory.

## Building and Publishing Locally

### Manual Publishing

To build ads-client and publish it to your local Maven repository:

```bash
./gradlew publishToMavenLocal
```

This command will:
1. Compile the Rust code for ads-client
2. Generate UniFFI bindings (Kotlin interfaces)
3. Generate Glean metrics code
4. Compile the Android/Kotlin code
5. Package the AAR file
6. Publish to `~/.m2/repository`

The build typically takes 10-20 minutes for a full build, or 1-3 minutes for incremental builds.

### Verify Publication

Check that ads-client was published successfully:

```bash
ls ~/.m2/repository/org/mozilla/appservices/ads-client/
```

You should see a version directory (e.g., `149.0a1`) containing the published artifacts.

## Testing with Firefox for Android (Fenix)

### Configure Fenix for Auto-Publishing

The recommended approach is to use the auto-publishing workflow, which automatically builds and publishes application-services when you build Fenix.

#### 1. Configure local.properties

Edit `mozilla-central/local.properties` (in the root of your mozilla-central checkout) and add:

```properties
autoPublish.application-services.dir=/absolute/path/to/application-services
```

Replace `/absolute/path/to/application-services` with the full path to your application-services checkout.

If the file doesn't exist, create it. It should already contain your `sdk.dir` configuration. The complete file will look like:

```properties
sdk.dir=/path/to/Android/sdk
autoPublish.application-services.dir=/absolute/path/to/application-services
```

#### 2. Disable Gradle Configuration Cache

The auto-publishing workflow requires disabling the Gradle configuration cache. You must disable it in **both** gradle.properties files:

**File 1:** `mozilla-central/gradle.properties` (root)

Find and comment out:
```properties
# org.gradle.configuration-cache=true
```

**File 2:** `mozilla-central/mobile/android/fenix/gradle.properties`

Find and comment out:
```properties
# org.gradle.configuration-cache=true
```

Both files must have this setting disabled for auto-publishing to work correctly. Fenix has its own gradle.properties that can override the root configuration.

### Building Fenix in Android Studio

1. Open Android Studio
2. Select **File → Open**
3. Navigate to `mozilla-central/mobile/android/fenix`
4. Click **Open**

When Android Studio syncs the project, it will:
- Detect the auto-publish configuration
- Build application-services from your local checkout
- Publish to local Maven
- Configure Fenix to use the local artifacts

The initial sync may take 10-20 minutes as it builds all components.

### Running Fenix

After the build completes:
1. Select your target device or emulator in Android Studio
2. Click the Run button (green play icon) or press `Shift + F10`
3. Fenix will launch with your local ads-client changes

## Troubleshooting

### Build Failures

**Problem:** Gradle can't find NDK
- **Solution:** Verify `ndk.dir` in `local.properties` points to the correct NDK version (29.0.14206865)

**Problem:** UniFFI binding errors or "Unresolved reference" errors
- **Solution:** Clean the build and regenerate:
  ```bash
  ./gradlew clean
  ./gradlew publishToMavenLocal
  ```

**Problem:** Package naming conflicts
- **Solution:** Ensure the following are consistent:
  - `uniffi.toml`: `package_name = "mozilla.appservices.adsclient"`
  - `android/build.gradle`: `namespace 'org.mozilla.appservices.adsclient'`
  - Directory structure: `android/src/main/java/mozilla/appservices/adsclient/`

### Auto-Publishing Not Working

If Fenix doesn't auto-publish application-services:

1. Verify `local.properties` uses an absolute path (not relative)
2. Confirm gradle configuration cache is disabled in `gradle.properties`
3. Try "Invalidate Caches and Restart" in Android Studio
4. Check the Android Studio

## Additional Resources

- [Main Application Services Building Guide](../../../docs/building.md)
- [Auto-Publishing Workflow Documentation](../../../docs/howtos/locally-published-components-in-fenix.md)
- [Android FAQs](../../../docs/android-faqs.md)
- [ads-client Usage Documentation](./usage.md)
