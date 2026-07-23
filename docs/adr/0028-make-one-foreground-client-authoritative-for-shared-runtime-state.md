---
status: accepted
---

# Keep one foreground client authoritative for host context

Oh My Herdr permits multiple thin clients to attach to one server, but at most one full app client is the global foreground client. The foreground client supplies shared host context: outer-terminal focus, the reported non-empty host terminal theme, the app-facing keybinding profile, and notification context. Direct terminal attach connections and pending terminal-attach clients are not full app clients for this arbitration.

ADR 0056 supersedes the earlier use of foreground ownership for shared pane size and interactive input. Per-tab PTY size, canonical terminal content, and interactive input authority now follow the stable tab's explicit controller. A watcher or a foreground client that is not that tab's controller cannot resize the tab or send its interactive input.

When there is no foreground app client, Oh My Herdr clears outer-terminal focus, applies server-owned keybindings, and shows server config diagnostics instead of client-local keybinding diagnostics. There is no foreground-owned fallback PTY size: each tab follows its Tab Control state under ADR 0056. Foreground synchronization remains part of the interactive wire-client runtime, not the JSON Local API control plane. Notification forwarding may use foreground host context for terminal/system side effects, but tab control remains separate.

## Current rationale

`[INFERENCE]` One foreground host context keeps focus, theme, keybinding, and notification behavior deterministic across host terminal surfaces. Keeping dimensions and interactive input under per-tab Tab Control avoids making a differently sized watcher alter another client's canonical terminal layout or input authority.

## Consequences

New headless client behavior that changes host focus, host theme, app-facing keybindings, or notification context should route through foreground-client synchronization rather than independently mutating shared host context. Behavior that changes a tab's PTY size or sends interactive tab input must route through that tab's controller or an explicit takeover under ADR 0056.

New client modes should decide explicitly whether they are full app clients eligible for foreground host context or specialized clients such as direct terminal attach. They should not become a tab controller or gain tab input authority merely because they have a writer or receive frames.
