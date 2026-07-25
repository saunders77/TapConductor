// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "tauri-plugin-apple-audio-session",
    platforms: [.iOS(.v16)],
    products: [
        .library(
            name: "tauri-plugin-apple-audio-session",
            type: .static,
            targets: ["tauri-plugin-apple-audio-session"]
        )
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-apple-audio-session",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources"
        )
    ]
)
