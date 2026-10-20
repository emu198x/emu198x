// swift-tools-version: 6.0
import PackageDescription

// Emu198xCore is the Rust runtime catalogue, built by
// scripts/build-apple-xcframework.sh. The frameworks below are the system
// libraries rustc reports for it (recorded in apple/build/build.json).
let package = Package(
    name: "Emu198xKit",
    platforms: [.macOS(.v14), .tvOS(.v17)],
    products: [
        .library(name: "Emu198xKit", targets: ["Emu198xKit"]),
    ],
    targets: [
        .binaryTarget(name: "Emu198xCore", path: "../build/Emu198xCore.xcframework"),
        .target(
            name: "Emu198xKit",
            dependencies: ["Emu198xCore"],
            linkerSettings: [
                .linkedFramework("AudioToolbox"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("Foundation"),
                .linkedFramework("IOKit", .when(platforms: [.macOS])),
                .linkedFramework("AVFAudio", .when(platforms: [.tvOS])),
                .linkedLibrary("iconv"),
                .linkedLibrary("objc"),
            ]
        ),
        .testTarget(name: "Emu198xKitTests", dependencies: ["Emu198xKit"]),
    ]
)
