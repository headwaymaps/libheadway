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
        targets: ["Headway", "HeadwayFFI"]
      ),
    ],
    targets: [
      .binaryTarget(
        name: "HeadwayRS",
        // run `./common/bin/build-ios.sh` to produce this framework
        // re-run whenever rust code is modified
        path: "./common/target/ios/libheadway-rs.xcframework"
      ),
      .target(
        name: "HeadwayFFI",
        dependencies: [.target(name: "HeadwayRS")],
        path: "apple/Sources/UniFFI"
      ),
      .target(
        name: "Headway",
        dependencies: [.target(name: "HeadwayFFI")],
        path: "apple/Sources/Headway"
      ),
    ]
)
