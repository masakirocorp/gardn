# libghostty-vt local patches

This file tracks intentional local changes applied on top of the vendored
`libghostty-vt` source. Remove a patch only when the vendored source commit
contains the upstream behavior and the listed verification still passes.

## 0001 default lib-vt panes to grapheme clustering

status: active

patch: `apps/gardn/vendor/patches/libghostty-vt/0001-default-grapheme-cluster-mode.patch`

upstream issue: https://github.com/ogulcancelik/herdr/issues/243

upstream discussion: not opened; libghostty-vt currently exposes current mode mutation but no C API for configuring terminal default modes

upstream PR: not opened

vendored base: `c5a21edfcbc2d5b46540ad91b7980aca31f5f1f3`

local files:

- `apps/gardn/vendor/libghostty-vt/src/terminal/c/terminal.zig`

reason: Gardn renders terminal cells directly and requires DEC private mode
2027 to store flags, ZWJ emoji, and other multi-codepoint grapheme clusters in
one cell. This patch makes clustering active for new terminals and keeps it as
the reset default so RIS (`ESC c`) does not disable it.

remove when: libghostty-vt exposes a C API for setting default mode 2027, or
upstream makes grapheme clustering the lib-vt default, and the reset-survival
regression passes without this patch.

verification:

```sh
cargo nextest run --locked grapheme_cluster_mode_is_default_and_survives_full_reset
cargo nextest run --locked grapheme_cluster_mode_renders_flag_emoji_in_single_wide_cell
cargo nextest run --locked grapheme_cluster_mode_renders_zwj_family_in_single_wide_cell
```

## 0002 skip unused Ghostty bench initialization

status: active

patch: `apps/gardn/vendor/patches/libghostty-vt/0002-skip-unused-ghostty-bench-init.patch`

upstream discussion: not opened; the upstream build initializes all named
artifacts before deciding which ones to install

vendored base: `c5a21edfcbc2d5b46540ad91b7980aca31f5f1f3`

local files:

- `apps/gardn/vendor/libghostty-vt/build.zig`

reason: Gardn builds only `-Demit-lib-vt`. Unconditional
`GhosttyBench.init` resolves unused dcimgui, vaxis, and zf packages. These
packages fetch ImGui and zigimg from GitHub and make CI depend on unrelated
network downloads.

remove when: the vendored build initializes GhosttyBench only for
`-Demit-bench`, or the build graph otherwise avoids resolving bench-only
packages for `-Demit-lib-vt`.

verification:

```sh
python3 -m unittest scripts.test_vendor_libghostty_vt
(cd apps/gardn/vendor/libghostty-vt && ZIG_GLOBAL_CACHE_DIR=$(mktemp -d) zig build -Demit-lib-vt -Doptimize=ReleaseFast -Dsimd=true)
```
