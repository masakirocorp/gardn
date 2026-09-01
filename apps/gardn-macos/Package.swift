// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "gardn-macos",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "GardnMenu", targets: ["GardnMenu"]),
    ],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.8.1"),
    ],
    targets: [
        .executableTarget(
            name: "GardnMenu",
            dependencies: [
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/GardnMenu",
            resources: [.process("Resources")]
        ),
    ]
)
