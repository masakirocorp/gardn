---
status: accepted
---

# Cache Kitty graphics by host view state

Hako treats Kitty graphics synchronization as Host Graphics cache reconciliation, not as normal text rendering and not as terminal-core ownership. The terminal core reports visible image placements, while `src/kitty_graphics.rs` owns the client-host side effects that upload image data, display placements, delete stale placements, and clear host image state when the app is no longer in terminal mode or cannot compute a usable cell size.

Uploaded images and displayed placements are cached separately. Image cache entries are keyed by host image id and an image signature derived from dimensions, format, data length, and data fingerprint; unchanged images are not re-uploaded. Placement cache entries are keyed by host image id plus host placement id and include clipped geometry, source rectangle, offsets, z-index, and scrollback offset; unchanged placements are suppressed only while the active host view key is unchanged.

The host view key is the active workspace index plus active tab index. A workspace/tab change can repaint the same terminal cells with text or overlays without deleting host-side Kitty placements, so Hako re-displays otherwise unchanged placements after a view change. Scrolling changes the placement signature through `scrollback_offset`, so a visible image is re-displayed when the viewport moves even if its terminal image data and grid geometry are unchanged.

Changed image data normally maps to a different host image id because the host id includes the image signature. If a cached host id ever has a different signature, Hako defensively deletes that old host image and removes its placements before uploading the replacement. When a cached placement is no longer visible, Hako emits a placement delete and removes only that placement cache entry; uploaded image data may remain cached until a surface reset. A full surface reset, non-terminal mode, or unknown cell size clears the cache by deleting cached host images and dropping remembered images, placements, and view state.

This is separate from ADR 0002's render-purity boundary: Kitty graphics writes are host-terminal side effects performed after view computation, not `AppState` mutations during draw. It is also separate from ADR 0014's terminal-core boundary: libghostty-vt supplies placement data through Hako's Rust wrapper, but Hako decides how to reconcile that data with the host terminal surface for the local app or each attached client.

## Current rationale

`[INFERENCE]` Hako caches images and placements because blindly re-uploading every visible image every frame would make graphics repaint expensive and noisy, while permanently suppressing unchanged placements would leave stale host-terminal graphics behind when Hako changes tabs, switches workspaces, scrolls, or repaints over prior graphics with normal terminal text.

## Consequences

New host graphics behavior should preserve the image/placement split. Image-data reuse should be decided from image signatures; placement display should be decided from placement signatures plus host view changes.

New graphics invalidation paths should either delete stale placements narrowly or clear all cached host images when the host surface can no longer be trusted. They should not rely on the terminal core to clean up host-terminal graphics that Hako displayed outside the core.

Additional terminal graphics protocols should use a similarly explicit host-surface reconciliation model or record a later ADR that replaces this one.
