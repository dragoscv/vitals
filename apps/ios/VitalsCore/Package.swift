// swift-tools-version: 6.0
// VitalsCore: everything the iPhone app does that is not a screen — the wire
// model mirrored from crates/vitals-core, the keyframe/delta merge, pairing
// checks, the LAN client and formatting. Pure Swift + Foundation so
// `swift test` runs on any Mac without a Simulator (vmui-mac has none).
import PackageDescription

let package = Package(
    name: "VitalsCore",
    defaultLocalization: "en",
    platforms: [.iOS(.v18), .macOS(.v14)],
    products: [
        .library(name: "VitalsCore", targets: ["VitalsCore"])
    ],
    targets: [
        .target(name: "VitalsCore"),
        .testTarget(
            name: "VitalsCoreTests",
            dependencies: ["VitalsCore"],
            resources: [.copy("Contract")]
        ),
    ],
    swiftLanguageModes: [.v6]
)
