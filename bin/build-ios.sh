#!/usr/bin/env bash

# NOTE: You MUST run this every time you make changes to `common`. Unfortunately, calling this from Xcode directly
# does not work so well.
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

set -e
set -u
set -x

crate=headway
rs_fw_name="${crate^}Rs" # e.g. headway -> HeadwayRs

# Must match the .iOS(.vXX) platform in Package.swift
export IPHONEOS_DEPLOYMENT_TARGET=16.0

cd "$SCRIPT_DIR/../common"

workdir=target/ios-workdir

create_fat_simulator_lib() {
  echo "Creating a fat library for x86_64 and aarch64 simulators" >&2
  cargo build -p $crate --lib --release --target aarch64-apple-ios-sim
  cargo build -p $crate --lib --release --target x86_64-apple-ios

  FAT_SIMULATOR_LIB_DIR="${workdir}/ios-simulator-fat/release"
  local output="$FAT_SIMULATOR_LIB_DIR/lib${crate}.a"
  mkdir -p "$FAT_SIMULATOR_LIB_DIR"
  lipo -create \
      target/x86_64-apple-ios/release/lib${crate}.a \
      target/aarch64-apple-ios-sim/release/lib${crate}.a \
      -output "$output"
  echo $output
}

create_ios_lib() {
  echo "Building $crate for iOS" >&2
  cargo build -p $crate --lib --release --target aarch64-apple-ios
  echo "target/aarch64-apple-ios/release/lib${crate}.a"
}

generate_uniffi() {
  echo "Generating Swift bindings from compiled rust library"
  cargo run -p uniffi-bindgen-swift -- "$1" ../apple/Sources/UniFFI --swift-sources
  # HACK: it seems like uniffi-bindgen-swift doesn't accept the ffi-module option, so we use good-ole sed
  sed -i '' 's/headwayFFI/HeadwayRs/g' ../apple/Sources/UniFFI/*.swift
}

build_framework_template() {
  framework_template_dir="${workdir}/uniffi-framework-template"

  echo "Generating .h from compiled rust library" >&2
  cargo run -p uniffi-bindgen-swift -- "$ios_lib_path" "${framework_template_dir}/Headers" --headers

  echo "Generating module map from compiled rust library" >&2
  cargo run -p uniffi-bindgen-swift -- "$ios_lib_path" "${framework_template_dir}/Modules" --xcframework --modulemap --modulemap-filename module.modulemap --module-name=$rs_fw_name

  cat > "${framework_template_dir}/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${rs_fw_name}</string>
    <key>CFBundleIdentifier</key>
    <string>earth.maps.${rs_fw_name}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${rs_fw_name}</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>MinimumOSVersion</key>
    <string>${IPHONEOS_DEPLOYMENT_TARGET}</string>
</dict>
</plist>
EOF
  echo "$framework_template_dir"
}

assemble_framework() {
  # Assembles a .framework bundle from a static library and headers.
  local fw_template="$1"
  local lib_path="$2"
  local parent_dir="$3"
  local fw_name="$4"

  local output_framework="${parent_dir}/${fw_name}.framework"

  mkdir -p "$parent_dir"
  cp -r "$fw_template" "$output_framework"
  cp "$lib_path" "$output_framework/${fw_name}"
}

build_xcframework() {
  echo "Generating XCFramework"

  local staging="${workdir}/staging"
  local output="${workdir}/${rs_fw_name}.xcframework"
  rm -rf "$staging" "$output"
  mkdir -p "$staging"

  local framework_template="$(build_framework_template)"

  assemble_framework \
    "$framework_template" \
    "$1" \
    "${staging}/ios-arm64/" \
    "$rs_fw_name"

  assemble_framework \
    "$framework_template" \
    "$2" \
    "${staging}/ios-arm64_x86_64-simulator/" \
    "$rs_fw_name"

  xcodebuild -create-xcframework \
    -framework "${staging}/ios-arm64/${rs_fw_name}.framework" \
    -framework "${staging}/ios-arm64_x86_64-simulator/${rs_fw_name}.framework" \
    -output "$output"
}

ios_lib_path="$(create_ios_lib)"
generate_uniffi $ios_lib_path
fat_sim_lib_path="$(create_fat_simulator_lib)"
build_xcframework $ios_lib_path $fat_sim_lib_path
