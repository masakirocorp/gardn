// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "gardn-macos",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "GardnMenu", targets: ["GardnMenu"]),
    ],
    targets: [
        .executableTarget(
            name: "GardnMenu",
            path: "Sources/GardnMenu",
            resources: [.process("Resources")]
        ),
    ]
)
