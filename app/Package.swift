// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "VoiceToText",
    platforms: [.macOS(.v13)],
    targets: [
        .target(name: "ObjCSupport"),
        .executableTarget(name: "VoiceToText", dependencies: ["ObjCSupport"]),
    ]
)
