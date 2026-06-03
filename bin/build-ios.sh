#!/usr/bin/env bash

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)

export IPHONEOS_DEPLOYMENT_TARGET=16.0

set -e
set -u

# Based on ferrostar/common/build-ios.sh

# NOTE: You MUST run this every time you make changes to `common`. Unfortunately, calling this from Xcode directly
# does not work so well.

# In release mode, we create a ZIP archive of the xcframework and update Package.swift with the computed checksum.
# This is only needed when cutting a new release, not for local development.
release=false
ffi_only=false

for arg in "$@"
do
    case $arg in
        --release)
            release=true
            shift # Remove --release from processing
            ;;
        --ffi-only)
            ffi_only=true
            shift # Remove --ffi-only from processing
            ;;
        *)
            shift # Ignore other argument from processing
            ;;
    esac
done

cd "$SCRIPT_DIR/../common"

header_dir=target/ios/framework-headers
generate_ffi() {
  # NOTE: During the xcode build process, headers from included frameworks are merged into a flat namespace,
  # so any other framework with a module.modulemap at the top level of includes would collide if we didn't
  # add a subdirectory for namespacing.
  #
  # This subdir's name must exactly match the name of the binary target in order for the subsequent xcode build
  # process to find these includes.
  #
  # e.g. if the includes end up in "Headers/headwayFFI", then our Package.swift must be like:
  #
  # ```
  # .binaryTarget(
  #   name: "headwayFFI", // <-- this name must match namespaced_header_dir
  #   path: "./common/target/ios/headwayFFI.xcframework"
  # ),
  # ```
  #
  # Technically the target name, framework file name, and module name are separate things,
  # but it's easier to keep everything straight when they are all the same.
  local module_name="${1}FFI"
  local namespaced_header_dir="${header_dir}/${module_name}"
  echo "Generating C-header and module map for ${module_name}" >&2
  # NOTE: Swift package managers clang invocation will only find the modulemap if it's named module.modulemap
  cargo run -p uniffi-bindgen-swift -- "target/aarch64-apple-ios/release/lib${1}.a" $namespaced_header_dir --headers --modulemap --module-name "$module_name" --modulemap-filename module.modulemap

  # NOTE: we use the aarch64-apple-ios target for the generated bindings.
  echo "Generating swift bindings for FFI" >&2
  cargo run -p uniffi-bindgen-swift -- target/aarch64-apple-ios/release/lib${1}.a ../apple/Sources/UniFFI --swift-sources
}

create_fat_simulator_lib() {
  # Potential optimizations for the future:
  #
  # * Only build one simulator arch for local development (we build both since many still use Intel Macs)
  # * Option to do debug builds instead for local development
  fat_simulator_lib_dir="target/ios/simulator-fat/release"

  echo "Creating a fat library for aarch64 and x86_64 simulators" >&2
  cargo build -p $1 --lib --release --target aarch64-apple-ios-sim
  cargo build -p $1 --lib --release --target x86_64-apple-ios
  mkdir -p $fat_simulator_lib_dir
  local output="${fat_simulator_lib_dir}/lib${1}.a"
  lipo -create target/x86_64-apple-ios/release/lib$1.a target/aarch64-apple-ios-sim/release/lib$1.a -output "$output"
  echo "$output"
}

build_xcframework() {
  echo "Generating XCFramework" >&2

  local simulator_lib="$(create_fat_simulator_lib $1)"

  xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/lib${1}.a -headers "$header_dir" \
    -library "$simulator_lib" -headers "$header_dir" \
    -output target/ios/${1}FFI.xcframework

  # NOTE: I've tried to keep the `release` code in sync with other changes, but haven't tested it.
  if $release; then
    echo "Building xcframework archive" >&2
    ditto -c -k --sequesterRsrc --keepParent target/ios/${1}FFI.xcframework target/ios/${1}FFI.xcframework.zip
    checksum=$(swift package compute-checksum target/ios/${1}FFI.xcframework.zip)
    version=$(cargo metadata --format-version 1 | jq -r --arg pkg_name "$1" '.packages[] | select(.name==$pkg_name) .version')
    sed -i "" -E "s/(let releaseTag = \")[^\"]+(\")/\1$version\2/g" ../Package.swift
    sed -i "" -E "s/(let releaseChecksum = \")[^\"]+(\")/\1$checksum\2/g" ../Package.swift
  fi
}

crate_name=headway

# Clean framework artifacts
rm -fr target/ios

echo "Building for iOS" >&2
cargo build -p $crate_name --lib --release --target aarch64-apple-ios

generate_ffi $crate_name

if $ffi_only; then
  echo "FFI-only build completed. Skipping XCFramework generation." >&2
  exit 0
fi

build_xcframework $crate_name
