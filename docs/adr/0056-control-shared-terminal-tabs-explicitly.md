---
status: accepted
---

# Control shared terminal tabs explicitly

Gardn assigns interactive control per stable tab identity, not per globally foreground app client. Each tab has at most one normal app client as its controller. The first client may claim a free tab, and switching to another free tab may claim that tab. An occupied tab is view-only for other normal app clients until they explicitly take over with `prefix+t` or the persistent Take control action in the desktop or mobile UI.

This complements ADR 0052's split between shared session state and client view state: Tab Control is the runtime coordination boundary for the one tab that multiple independent views may observe.

The controller establishes the tab's canonical PTY size and is the interactive input authority. A watcher renders the tab's canonical terminal canvas in its own viewport by cropping or padding it to fit. Watchers keep navigation, scroll, copy, and search state local to their client. Watcher focus, resize, or input does not resize the PTY or change the tab's terminal content. A watcher sees no layout shift merely because its viewport differs; only explicit takeover can change the controller and canonical size.

Control is released when the controller navigates away from the tab, disconnects, or direct-attaches the terminal. A released tab is not automatically assigned to a watcher. A watcher must explicitly claim or take over control before its size or input can affect the tab.

This interactive ownership boundary does not apply to automation. The Local API and system automation operate through their control-plane contracts and do not need to claim a tab controller. Direct terminal attach remains exclusive at the terminal level under ADR 0011; its explicit terminal takeover is separate from normal app-client Tab Control. The global foreground client retained by ADR 0028 supplies host focus, host theme, app-facing keybinding, and notification context only. It does not own per-tab PTY sizing or interactive input authority.

## Current rationale

`[INFERENCE]` Per-tab control prevents a watcher with a different desktop or mobile size from changing the controller's PTY and shifting the shared layout. Requiring an explicit takeover makes a change of interactive authority visible and reversible, while declining to auto-promote watchers avoids surprising control changes after navigation or disconnect.

## Consequences

Normal app clients must identify the stable tab whose control they are requesting and render controller/watcher state clearly enough for an explicit takeover. View-only operations remain client-local; operations that resize or send interactive tab input require the controller or an explicit takeover. Disconnect and navigation handling must release control without selecting a replacement watcher. Local API and system automation paths must remain independent of this interactive ownership model, and direct terminal attach must continue to enforce its separate terminal-level exclusivity.
