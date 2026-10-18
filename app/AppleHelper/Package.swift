// swift-tools-version:6.2
// ovt-apple: Apple's on-device engines (SpeechAnalyzer, Foundation Models) for dictate.sh and the app.
// A package of its own, because it needs the macOS 26 SDK and Swift 6.2: the app's package keeps building with older
// toolchains, just without this helper. See docs/plans/M5.6-apple-on-device.md.
import PackageDescription

let package = Package(
    name: "AppleHelper",
    platforms: [.macOS("26.0")],
    targets: [
        .executableTarget(
            name: "ovt-apple",
            path: "Sources/ovt-apple",
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
