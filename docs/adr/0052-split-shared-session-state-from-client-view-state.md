---
status: accepted
---

# Split shared session state from client view state

Hako will support multiple normal app clients attached to one server as one shared session with independent client views. The shared session owns durable workspace and runtime-adjacent product state; each normal app client owns its own navigation, transient UI state, computed geometry, and render/input surface state. A client can look at a different workspace, tab, pane, modal, sidebar scroll position, command palette query, or terminal scrollback viewport without changing another client's view.

This refines ADR 0028 rather than replacing it. One foreground app client remains authoritative for shared runtime context that cannot be merged safely: pane runtime/effective size, outer-terminal focus, host terminal theme, and active keybinding profile. Independent client views must not make every attached terminal surface resize PTYs or race over host-context fields. Direct terminal attach clients remain outside the normal app view model under ADR 0011 and ADR 0029.

## Ownership model

Shared session state owns structural and durable data that should converge for every client:

- workspace groups, workspaces, tabs, pane layout trees, pane records, public pane aliases, and workspace/group identity;
- terminal metadata, agent state evidence, integration metadata, command runs, plugin command logs, port observations, Git work summaries, and pending terminal/runtime shutdowns;
- committed config and settings values, agent profile catalog, installed plugin registry, update/release state, session dirty state, persistence/handoff metadata, and API-visible status;
- runtime coordination that is intentionally single-owner, including foreground client id, direct attach owners, direct attach resize locks, effective runtime size, foreground host focus/theme, and server/client keybinding arbitration.

Client view state owns user-surface state that can differ between clients without changing the shared session:

- selected workspace, active group/filter navigation, active tab per workspace, focused pane per tab, zoom/focus history, current app mode, navigator state, copy mode, and selected text context;
- sidebar collapse, right-sidebar collapse, sidebar widths where user-surface local, workspace/agent/tab/mobile scroll offsets, hovered/pressed rows, drag state, context menus, global/group/agent menus, keybind help scroll, release-note/product-announcement view state, toast hit/click state, copy feedback, and selection autoscroll;
- settings modal transient state: section, selected row, scroll, draft values, original values, pending target rows, and unsaved text inputs;
- command palette, agent profile picker, Git repo picker, group/new-tab/rename/delete modal drafts, icon picker state, and any other modal draft or selection;
- computed geometry and render-only hit areas currently represented by `ViewState`;
- terminal scrollback viewport offsets when those are view choices rather than shared document data.

Client surface state stays per connection, not in shared session state: terminal size, cell size, negotiated render encoding, render baseline, render pending flag, Kitty graphics cache, host mouse-capture mode, staged clipboard-image files, and writer channels. Much of this already lives on `ClientConnection`; the missing seam is the normal app view/navigation state that still lives in `AppState`.

## Construction and reconciliation

A new app client starts with a default view cloned from the current foreground/default view when one exists, otherwise from a deterministic projection of shared session state and config. Restore and handoff persist shared session data plus one default/foreground view, not every transient attached client view. Old snapshots migrate into that default view so users restore the same visible workspace, tab, and pane they had before the split.

Structural mutations stay shared. Creating, deleting, moving, or renaming workspaces/tabs/panes updates the shared session and then reconciles every live client view to valid ids. Reconciliation should prefer stable public/internal ids over raw indices; indices are acceptable only as cached projections rebuilt from shared state. If a viewed object disappears, the client falls back to the nearest valid sibling, then the active/default workspace, then the built-in empty state.

## Input, render, and API rules

ADR 0010 still applies: normal thin clients forward bytes and the server remains the semantic input owner. The input route must include the source client so navigation/view actions mutate that client's view, while shared object operations mutate the shared session. Direct terminal attach input still bypasses normal app semantic routing.

ADR 0002 still applies: render remains read-only. View computation may reconcile client-local geometry and scroll bounds before drawing; drawing takes immutable shared state plus immutable client view state. Background client renders must not resize shared PTYs. Foreground-client synchronization remains the only path that mutates shared runtime size, focus, host theme, or keybinding context.

ADR 0013 still applies: the Local API is the control plane. API calls with explicit workspace/tab/pane ids are canonical and do not depend on a client view. Legacy no-target API/plugin operations use the foreground/default view only as a compatibility fallback, and plugin event context should carry the invoking client/view when a plugin action is view-sensitive.

Notifications remain centralized unless a later ADR changes delivery policy. View-sensitive suppression and toast click handling must be evaluated against the relevant client view or foreground pane, and a toast click in one client must not navigate another client.

## Terminal viewport split

Terminal runtime output, parser state, history buffers, and pane terminal identity stay shared runtime/session data under ADR 0001. A client's scrollback viewport and search/navigation position are client view state unless Hako intentionally exposes a shared follow/observe mode later.

## MVP transition sequence

1. Add `ClientViewState` and default-view construction while preserving one-client behavior.
2. Render from shared session state plus explicit client view state.
3. Route semantic input with source client/view context.
4. Move active workspace/tab/pane/mode/sidebar/modal state out of shared `AppState`.
5. Split terminal viewport state after the general view seam exists.
6. Update persistence, restore, handoff, notifications, API/plugin fallbacks, and direct attach audits.
7. Remove projection adapters, global compatibility fields, and stale test helpers.

## Current rationale

`[INFERENCE]` Users expect multiple attached Hako app clients to behave like multiple views into one workspace manager, not a screen share where every navigation action yanks other clients around. The current code already treats render streams and client surface data as per-client, but app navigation state remains shared in `AppState`, so the implementation is halfway between independent views and shared screen mirroring. Splitting shared session state from client view state completes the existing direction without moving terminal/runtime ownership into clients.

## Consequences

New app navigation features must choose an owner explicitly: shared session, client view, client surface, or runtime coordination. Fields that only describe what one attached client is looking at belong in client view state. Fields that describe durable workspace objects, terminal identity, committed settings, API-visible state, or real runtime ownership belong in shared session state or runtime coordination.

Tests for multi-client behavior should assert user-visible independence: two clients can select different workspaces/tabs/panes, open different modals, scroll different terminal views, and render at different sizes without mutating each other's views. Tests for shared behavior should assert convergence: workspace structure, terminal output, committed settings, Git mutations, command runs, and agent state changes remain shared.

The split should not introduce a wire-protocol bump until a client-visible protocol field changes. Internal server routing can carry client ids and view state without changing framed bytes. If later work persists multiple named client views or exposes view ids over the API, that will need its own compatibility review.
