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

vendored base: `5834a0e3df621802e9578e4562d88b0c2ad4ada8`

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

vendored base: `5834a0e3df621802e9578e4562d88b0c2ad4ada8`

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

## 0003 preserve Kitty graphics in snapshots

status: active

patch: `apps/gardn/vendor/patches/libghostty-vt/0003-preserve-kitty-graphics-in-snapshots.patch`

upstream discussion: not opened

vendored base: `5834a0e3df621802e9578e4562d88b0c2ad4ada8`

local files:

- `apps/gardn/vendor/libghostty-vt/include/ghostty/vt/snapshot.h`
- `apps/gardn/vendor/libghostty-vt/include/ghostty/vt/terminal.h`
- `apps/gardn/vendor/libghostty-vt/src/terminal/c/render.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/c/terminal.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/render.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/envelope.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/history.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/kitty.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/main.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/screen.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/snapshot.ksy`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/snapshot.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/terminal.zig`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/testdata/complete-v2.hex`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/testdata/envelope-v2.hex`
- `apps/gardn/vendor/libghostty-vt/src/terminal/snapshot/verify-kaitai.py`

reason: Remote checkpoint recovery must retain image pixels, placements,
animation state, and unfinished uploads. SCREEN includes older pages when
retained image pins require them. Decode reconstructs pins and rejects
aggregate image data that exceeds the destination storage limit. It does not
restore source filesystem permissions or temporary-directory paths.
Native color override options let Gardn clear abandoned child overrides without
injecting escape sequences into an unfinished terminal parser.
Render state resolves foreground and background independently. An unset color
must not hide an override on the other color.

The native format is version 2. Decoders reject version 1. Gardn includes this
change in unreleased Execution Worker Protocol version 3.

remove when: Upstream snapshots preserve the same graphics state and
destination-owned policy, and the recovery regressions pass without this patch.

verification:

```sh
python3 -m unittest scripts.test_vendor_libghostty_vt
(cd apps/gardn/vendor/libghostty-vt && zig build test-lib-vt -Dtest-filter=snapshot)
(cd apps/gardn/vendor/libghostty-vt && zig build test-lib-vt -Dtest-filter='clearing color overrides preserves defaults and pending OSC input')
(cd apps/gardn/vendor/libghostty-vt && zig build test-lib-vt -Dtest-filter='render: independent default colors and reverse mode')
(cd apps/gardn/vendor/libghostty-vt && src/terminal/snapshot/verify-kaitai.py)
```
