// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "Headway",
    platforms: [
      .iOS(.v16),
    ],
    products: [
      .library(
        name: "Headway",
        targets: ["Headway", "HeadwayUniFFI"]
      ),
    ],
    targets: [
      .binaryTarget(
        name: "headwayFFI",
        // run `./bin/build-ios.sh` to produce this framework
        // re-run whenever rust code is modified
        path: "./common/target/ios/headwayFFI.xcframework"
      ),
      .target(
        name: "HeadwayUniFFI",
        dependencies: [.target(name: "headwayFFI")],
        path: "apple/Sources/UniFFI"
      ),
      .target(
        name: "Headway",
        dependencies: [.target(name: "HeadwayUniFFI")],
        path: "apple/Sources/Headway"
      ),
    ]
)
