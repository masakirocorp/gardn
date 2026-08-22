---
status: accepted
---

# Restore host terminal theme after transient overrides

Gardn caches the host terminal theme applied to a pane and restores it after a foreground child process temporarily changes default foreground or background colors. The restore path records the non-shell foreground process group observed when transient default-color OSC sequences arrive, waits until the shell is foreground on the primary screen, and, off macOS, also requires the foreground process group to differ from the recorded owner.

This treats child default-color changes as temporary presentation effects, not as lasting Gardn theme authority. The guard matters because Gardn also has a foreground-client theme authority model: host theme reports choose the theme Gardn should use, while pane-level OSC tracking decides when a child process has temporarily overridden that theme and when it is safe to write the cached colors back.

This is separate from ADR 0028. ADR 0028 records which client owns shared host theme state; this ADR records per-pane recovery from child processes that write default-color OSC sequences after that host theme has been applied.

## Current rationale

`[INFERENCE]` Gardn restores cached host colors so transient full-screen or agent programs do not leave the user's terminal in an unexpected color state after returning to the shell. Waiting for shell foreground and primary-screen state avoids fighting the foreground program while it still owns the visual surface.

## Consequences

New host-theme behavior should distinguish theme authority from transient child overrides. Restoration should remain guarded by foreground-job and screen-mode evidence, and child OSC changes should not silently become the new durable Gardn theme.
