# XCFramework Collision Fix — Work Log

## Problem

The maps.earth app depends on two Rust FFI libraries, via uniffi. Both built as library-style
XCFrameworks via `bin/build-ios.sh`:

- `libferrostar-rs.xcframework` (from ferrostar)
- `libheadway-rs.xcframework` (from headway)

Both XCFrameworks include a `module.modulemap` at the root of their `Headers/`
directory. Xcode's `ProcessXCFramework` build step copies each XCFramework's
headers into a single shared flat directory (`$(BUILD_PRODUCTS_DIR)/include/`),
so both `module.modulemap` files get written to the same path.

### Original error

```
Multiple commands produce
  '.../Build/Products/Debug-iphonesimulator/include/module.modulemap'

Command: ProcessXCFramework .../libferrostar-rs.xcframework
Command: ProcessXCFramework .../libheadway-rs.xcframework
```

---

## Attempt 1: Namespace headers in a subdirectory (library-style)

**Idea:** Keep headway as library-style but place the generated headers in a
per-library subdirectory (`headers/headway/`) and pass the *parent* directory
as `-headers`. ProcessXCFramework would then copy `headway/module.modulemap`
into `include/headway/module.modulemap`, not `include/module.modulemap`.

**Result:** No collision, but the compiler never finds the modulemap. The Clang
module system only searches for `module.modulemap` at the root of each include
path directory — not in subdirectories.

**Error:**
```
Cannot find type 'RustBuffer' in scope
Cannot find type 'ForeignBytes' in scope
```

---

## Attempt 2: Framework-style XCFramework named `headwayFFI.framework`

**Idea:** Switch headway from library-style (`-library`) to framework-style
(`-framework`). Each slice becomes a `.framework` bundle with its own
`Modules/module.modulemap`, so nothing is copied into the shared `include/`.
The framework must be named after its C module (`headwayFFI`) so that
`import headwayFFI` resolves to `headwayFFI.framework/Modules/module.modulemap`.

**Result:** The framework name `headwayFFI` case-insensitively collides with the
SPM Swift target `HeadwayFFI`. macOS has a case-insensitive filesystem, so
`headwayFFI.framework` and `HeadwayFFI.framework` resolve to the same path in
the build products. SPM compiles the Swift target `HeadwayFFI` into
`HeadwayFFI.framework`; the compiler then finds the C module there instead.

**Error:**
```
Cannot load module 'HeadwayFFI' as 'headwayFFI'
```

---

## Attempt 3: Framework-style, rename Swift target to avoid case collision

**Idea:** The root cause of Attempt 2's failure is that the Swift SPM target
`HeadwayFFI` and the C module `headwayFFI` share the same name modulo
case. Rename the Swift target from `HeadwayFFI` to `HeadwayUniFFI` (or
similar) so the two names are unambiguously different even on a
case-insensitive filesystem. Then framework-style works cleanly:

- C module: `headwayFFI` → `headwayFFI.framework` ✓
- Swift target: `HeadwayUniFFI` → `HeadwayUniFFI.framework` ✓ (no collision)

**Changes needed:**
- `Package.swift`: rename target `HeadwayFFI` → `HeadwayUniFFI`, update product and dependency references
- Any Swift source files that `import HeadwayFFI` (as the Swift module) → `import HeadwayUniFFI`
- `bin/build-ios.sh`: restore framework-style `build_xcframework`

**Complication:** Even after renaming the Swift target, stale DerivedData artifacts
(`HeadwayFFI.swiftmodule`, `HeadwayFFI.modulemap`, etc.) from before the rename
caused the same error to persist. A clean DerivedData wipe was required.

The rename was verified as genuinely necessary: reverting it and running
`bin/clean && bin/build-and-test` (fully clean, no DerivedData) reproduces
the error immediately.

**Resolution:** ✅ Tests pass — but `HeadwayUniFFI` was an ugly name.

---

## Attempt 4: Uppercase the C framework, rename Swift target to HeadwayCore

**Idea:** Keep `HeadwayFFI` as the C framework name but uppercase it
(`HeadwayFFI.framework` instead of `headwayFFI.framework`), and rename the
Swift target to the clean `HeadwayCore`. This avoids the case collision:

- `HeadwayFFI.framework` (C module) → case-insensitive: `headwayffi`
- `HeadwayCore.framework` (Swift target) → case-insensitive: `headwaycore` ✓

Since uniffi generates `import headwayFFI` (lowercase) in Swift source, a
`sed` in `generate_uniffi` patches it to `import HeadwayFFI` after generation.
`--module-name` is also set to `HeadwayFFI` to match.

**Final naming:**
- `Headway` — handwritten Swift API
- `HeadwayCore` — uniffi-generated Swift wrapper (SPM target, `apple/Sources/UniFFI/`)
- `HeadwayFFI` — C framework from the Rust library (framework-style XCFramework)
- `HeadwayRS` — SPM binary target wrapping the XCFramework

**Resolution:** ✅ Tests pass.

---

## Attempt 5 (current): Rename C module to HeadwayRs, Swift target back to HeadwayUniFFI

**Motivation:** Attempt 4's naming was inconsistent and confusing:
- `HeadwayFFI` as the C module name recycled a name previously used for a Swift
  target, making it easy to confuse the C and Swift layers.
- `HeadwayCore` was a vague name for the auto-generated UniFFI wrapper.

**Idea:** Name the C module after the Rust crate, not the FFI concept.
The build script already computes `rs_fw_name="${crate^}Rs"` (e.g. `headway` →
`HeadwayRs`), giving a name that is clearly Rust-origin and avoids any collision
with Swift targets. The Swift UniFFI target is restored to `HeadwayUniFFI`, which
accurately describes what it is.

**Changes:**
- `--module-name` in `build-ios.sh` → `$rs_fw_name` = `HeadwayRs`
- `sed` patches uniffi-generated Swift from `headwayFFI` → `HeadwayRs`
- `Package.swift`: target `HeadwayCore` → `HeadwayUniFFI`
- XCFramework output: `HeadwayRs.xcframework` (in `common/target/ios-workdir/`)

**Final naming:**
- `Headway` — handwritten Swift API (`apple/Sources/Headway/`)
- `HeadwayUniFFI` — uniffi-generated Swift wrapper (`apple/Sources/UniFFI/`); imports `HeadwayRs`
- `HeadwayRs` — C module inside the framework-style XCFramework (raw FFI types)
- `HeadwayRS` — SPM binary target wrapping `HeadwayRs.xcframework`

**Resolution:** ✅ Current working state.
