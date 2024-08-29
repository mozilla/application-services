// swift-tools-version:5.6
import PackageDescription

let package = Package(
    name: "RustComponents",
    targets: [
        .target(
            name: "SwiftComponents",
            dependencies: [],
            path: "Sources/SwiftComponents",
            publicHeadersPath: "include", // SPM will look here for your module.modulemap
            cSettings: [
                .headerSearchPath("include"),
                .define("MODULE_MAP", to: "include/module.modulemap")
            ]
        ),
    ]
)
