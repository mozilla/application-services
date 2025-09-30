# How to locally test Swift Package Manager components on Firefox iOS

> This guide explains how to build and test **Firefox iOS against a local Application Services** checkout.
> For background on our Swift Package approach, see the [ADR](../adr/0003-swift-packaging.md).

---

## At a glance

**Goal:** Build a local Firefox iOS against a local Application Services.

**Current workflow (recommended):**

1. Build an **XCFramework** from your local `application-services`.
2. Point **Firefox iOS’s local Swift package** (`MozillaRustComponents/Package.swift`) at that artifact (either an HTTPS URL + checksum, **or** a local `path:`).
3. Reset package caches in Xcode and build Firefox iOS.

A legacy flow that uses the **`rust-components-swift`** package is documented at the end while we're in mid-transition to the new system.

---

## Prerequisites

1. A local checkout of **Firefox iOS** that builds: <https://github.com/mozilla-mobile/firefox-ios#building-the-code>
2. A local checkout of **Application Services** prepared for iOS builds: see [Building for Firefox iOS](../building.md#building-for-firefox-ios)

---

## Step 1 — Build the XCFramework from Application Services

From your `application-services` checkout:

```bash
cd megazords/ios-rust
./build-xcframework.sh
```

This produces MozillaRustComponents.xcframework.zip (and you can also unzip it to get MozillaRustComponents.xcframework) containing:

- The compiled Rust code as a static library (for all iOS targets)
- C headers and Swift module maps for the components

> Tip: If you plan to use the URL-based approach below, compute the checksum once you have the zip:

```bash
swift package compute-checksum MozillaRustComponents.xcframework.zip
```

## Step 2 — Point Firefox iOS to your local artifact

Firefox iOS consumes Application Services via a local Swift package in-repo at:

```
{path-to-firefox-ios}/MozillaRustComponents/Package.swift
```

update it in one of two ways:

### Option A: URL + checksum (zip artifact)

1. Host your MozillaRustComponents.xcframework.zip at an HTTPS-accessible URL (e.g., a Taskcluster or GitHub artifact URL).
2. Edit MozillaRustComponents/Package.swift and set the binaryTarget to the zip URL and checksum:

```swift
// In firefox-ios/MozillaRustComponents/Package.swift
.binaryTarget(
  name: "MozillaRustComponents",
  url: "https://example.com/path/MozillaRustComponents.xcframework.zip",
  checksum: "<sha256 from `swift package compute-checksum`>"
)
```

> Note: Every time you produce a new zip, you must update the checksum.

### Option B: Local path (fastest for iteration)

1. Unzip the XCFramework near the package (or anywhere on disk).
2. Switch the binaryTarget to a local path:

```swift
// In firefox-ios/MozillaRustComponents/Package.swift
.binaryTarget(
  name: "MozillaRustComponents",
  path: "./MozillaRustComponents.xcframework"
)
```

> UniFFI bindings: If your component requires UniFFI-generated Swift, ensure the package targets reference the directory where generated Swift files are emitted (same pattern used in the repo’s Package.swift today).

## Step 3 — Reset caches and build

In Xcode:

- File → Packages → Reset Package Caches
- (If needed) File → Packages → Update to Latest Package Versions
- Product → Clean Build Folder, then build and run Firefox iOS.

If you still see stale artifacts (rare), delete:

```swift
~/Library/Caches/org.swift.swiftpm
~/Library/Developer/Xcode/DerivedData/*
```

…and build again.

---

## Disabling local development

To revert quickly:

1. Restore your changes to MozillaRustComponents/Package.swift (e.g., git checkout -- MozillaRustComponents/Package.swift).
2. Reset Package Caches in Xcode.
3. Build Firefox iOS.

## Troubleshooting

- Old binary still in use: Reset caches and clear DerivedData, then rebuild.
- Branch switches in application-services: Rebuild the XCFramework and update the package reference (URL/checksum or path:).
- Checksum mismatch (URL mode): Run swift package compute-checksum on the new zip and update Package.swift.
- Build script issues: Re-run ./build-xcframework.sh from megazords/ios-rust.

## Legacy: using rust-components-swift (remote package)

[!WARNING]
Status: rust-components-swift is deprecated for Firefox iOS. Prefer the local package at MozillaRustComponents/ unless you must validate against the legacy package for a specific task.

Some teams may still need the legacy flow temporarily. Historically, Firefox iOS consumed Application Services through the rust-components-swift package. To test locally with that setup:

1. Build the XCFramework from application-services.
2. In a local checkout of rust-components-swift, point its Package.swift to the local path of the unzipped XCFramework:
   ```swift
   .binaryTarget(
   name: "MozillaRustComponents",
   path: "./MozillaRustComponents.xcframework"
   )
   ```
3. Commit the changes in rust-components-swift (Xcode only reads committed package content).
4. In Firefox iOS, replace the package dependency with a local reference to your rust-components-swift checkout (e.g., via Xcode’s “Add Local…” in Package Dependencies).
