// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "MozillaRustComponents",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "MozillaRustComponents", targets: ["MozillaRustComponentsWrapper"]),
    ],
    dependencies: [
        .package(url: "https://github.com/mozilla/glean-swift", from: "64.5.1"),
    ],
    targets: [
        // Binary target XCFramework, contains our rust binaries and headers
        .binaryTarget(
            name: "MozillaRustComponents",
            path: "MozillaRustComponents.xcframework"
        ),

        // A wrapper around our binary target that combines + any swift files we want to expose to the user
        .target(
            name: "MozillaRustComponentsWrapper",
            dependencies: ["MozillaRustComponents", .product(name: "Glean", package: "glean-swift")],
            path: "Sources/MozillaRustComponentsWrapper"
        ),

        // Tests
        .testTarget(
            name: "MozillaRustComponentsTests",
            dependencies: ["MozillaRustComponentsWrapper"]
        ),
    ]
)
