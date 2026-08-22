---
status: accepted
---

# Vendor libghostty-vt as terminal core

Gardn uses a pinned vendored `libghostty-vt` source tree as its terminal-emulation core. `scripts/vendor_libghostty_vt.py` copies a Ghostty `zig build dist -Demit-lib-vt -Doptimize=ReleaseFast` source distribution into `apps/gardn/vendor/libghostty-vt` and records `source_repo`, `source_commit`, `dist_archive`, and `extracted_dir` in `apps/gardn/vendor/libghostty-vt.vendor.json`. `source_repo` is local provenance; commit, archive, and extracted-dir identify the vendored source distribution.

Cargo builds the vendored source instead of linking a prebuilt terminal library. `apps/gardn/build.rs` parses `.minimum_zig_version` from `apps/gardn/vendor/libghostty-vt/build.zig.zon` and requires an exact matching Zig binary from `$ZIG`, `zig`, or Homebrew candidates. It runs `zig build -Demit-lib-vt` with Gardn's target, optimization, SIMD, and version-string settings, then links the resulting static library from `zig-out/lib`. `scripts/build_vendored_libghostty_vt.sh` is only a manual build helper inside the vendored directory; it does not reproduce Cargo's Zig resolution, target, SIMD, or version-string setup unless the caller supplies those flags. `scripts/test_vendor_libghostty_vt.py` checks required upstream files, metadata-key presence, and embedded logging-silencing strings.

Rust owns the Gardn boundary around that core. `apps/gardn/src/ghostty/` wraps checked-in bindgen output plus manually maintained Kitty graphics FFI shims/constants that must stay aligned with vendored headers. `apps/gardn/src/pane/terminal.rs` feeds PTY bytes into `crate::ghostty::Terminal`, tracks input/render state, orders terminal responses, and adapts rendering delays for features such as synchronized output and kitty graphics. Gardn also reconstructs host-facing behavior outside libghostty-vt when needed, such as forwarding OSC 52 clipboard writes because the vendored core drops `.clipboard_contents`.

## Current rationale

`[INFERENCE]` Gardn does not maintain its own terminal emulator because terminal compatibility, keyboard protocols, and graphics behavior are too large and subtle to reimplement as workspace-manager code. Gardn vendors a source distribution rather than tracking an external checkout or prebuilt artifact because the C/Zig API is still a boundary Gardn must pin, build reproducibly for release targets, and adapt with local Rust glue. Gardn keeps host-facing gaps outside the vendored tree so upstream terminal behavior can be refreshed without mixing Gardn product policy into the dependency.

## Consequences

The vendored source, vendoring metadata, exact Zig version, checked-in bindings, and manual binding shims are one dependency boundary. Updating libghostty-vt must update that boundary together and rerun the vendoring guard tests; changing terminal behavior by editing only Rust wrappers or only the vendored tree risks drift.

Build and release environments need the exact Zig version parsed from `apps/gardn/vendor/libghostty-vt/build.zig.zon`. That Zig pin can also constrain the surrounding platform toolchain: Zig `0.15.2` does not link Gardn's vendored libghostty-vt build against Xcode `26.4+` macOS SDK TBDs, so macOS CI/release jobs select Xcode `26.3` while still running on `macos-latest`. Remove that Xcode selection only after the vendored source builds with a Zig version that handles the newer SDK. Unsupported Rust targets fail at build time unless `build.rs::zig_target` maps them to a supported libghostty-vt target.
