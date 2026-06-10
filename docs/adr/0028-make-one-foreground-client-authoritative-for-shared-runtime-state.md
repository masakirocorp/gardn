---
status: accepted
---

# Make one foreground client authoritative for shared runtime state

Hako permits multiple thin clients to attach to one server, but the server derives shared app-facing runtime state from at most one foreground full app client. `HeadlessServer` tracks `foreground_client_id`; when present, that client drives the shared pane runtime/effective size, outer-terminal focus, reported non-empty host terminal theme, and active keybinding profile applied to the app. Direct terminal attach connections and pending terminal-attach clients are not full app clients for this arbitration.

Foreground ownership changes on user interaction and full app client resize. Key, mouse, paste, and outer-focus-gained events promote the sending app client; focus-loss alone updates that client's stored focus without promoting it. Resize from a full app client also promotes that client, because the shared pane runtime/effective size determines pane wrapping and view computation. New full app clients become foreground when they connect, and when the foreground client disconnects or detaches, Hako promotes the most recently active remaining full app client.

When there is no foreground app client, Hako falls back to minimum shared size, clears outer-terminal focus, applies server-owned keybindings, and shows server config diagnostics instead of client-local keybinding diagnostics. When a foreground client exists, Hako applies that client's size, focus, non-empty host theme, and either its local keybindings or the server keybindings when the client requested server mode. Shared runtime size changes force a full redraw for remaining clients because pane wrapping and foreground-driven rendering semantics may have changed even if a cached frame appears equal.

This is separate from ADR 0010's byte-framed input boundary: clients still forward input bytes and the server decodes normal app input, but one app client may be chosen as the owner of the shared host-surface context. It is separate from ADR 0013's local API/wire split: foreground ownership is part of the interactive wire-client runtime, not the JSON Local API control plane. It is also separate from ADR 0021's notification delivery: notification forwarding may use the foreground client for terminal/system side effects, but this ADR records the broader runtime surface authority that makes focus, size, theme, and keybindings deterministic.

## Current rationale

`[INFERENCE]` Hako picks one foreground app client because merging dimensions, focus, host theme, and keybinding profiles from multiple host terminal surfaces would produce unstable pane layout and ambiguous app input semantics. Using the most recently interactive full app client keeps the shared runtime aligned with the human-controlled surface while still allowing secondary clients to render and observe.

## Consequences

New headless client behavior that changes app-facing size, focus, host theme, or keybindings should route through foreground-client synchronization rather than independently mutating shared app state.

New client modes should decide explicitly whether they are full app clients eligible for foreground ownership or specialized clients such as direct terminal attach. They should not accidentally become foreground merely because they have a writer or receive frames.
