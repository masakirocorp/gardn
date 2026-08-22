---
status: accepted
---

# Split shared session state from client view state

Gardn supports multiple normal app clients attached to one server as one shared session with independent client views. The shared session owns durable workspace and runtime-adjacent product state; each normal app client owns its own navigation, transient UI state, computed geometry, and render/input surface state. A client can look at a different workspace, tab, pane, modal, sidebar scroll position, command palette query, or terminal scrollback viewport without changing another client's view. A watcher can view a tab controlled by another client without changing that tab's canonical terminal.

This refines ADR 0028 and works with ADR 0056. The global foreground client remains authoritative only for host focus, host terminal theme, app-facing keybindings, and notification context. Tab Control assigns one explicit controller per stable tab identity for canonical PTY size and interactive input; direct terminal attach clients remain outside the normal app view model under ADR 0011.

## Ownership model

Shared session state owns structural and durable data that should converge for every client:

- workspace groups, workspaces, tabs, pane layout trees, pane records, public pane aliases, and workspace/group identity;
- terminal metadata, agent state evidence, integration metadata, command runs, plugin command logs, port observations, Git work summaries, and pending terminal/runtime shutdowns;
- committed config and settings values, agent profile catalog, installed plugin registry, update/release state, session dirty state, persistence/handoff metadata, and API-visible status;
- runtime coordination that is intentionally single-owner, including one Tab Control slot per stable tab identity, direct attach owners, direct attach resize locks, each controller's canonical runtime size, foreground host focus/theme, and server/client keybinding and notification-context arbitration.

Client view state owns user-surface state that can differ between clients without changing the shared session:

- selected workspace, active group/filter navigation, active tab per workspace, focused pane per tab, zoom/focus history, current app mode, navigator state, copy mode, and selected text context;
- sidebar collapse, right-sidebar collapse, sidebar widths where user-surface local, workspace/agent/tab/mobile scroll offsets, hovered/pressed rows, drag state, context menus, global/group/agent menus, keybind help scroll, release-note/product-announcement view state, toast hit/click state, copy feedback, and selection autoscroll;
- settings modal transient state: section, selected row, scroll, draft values, original values, pending target rows, and unsaved text inputs;
- command palette, agent profile picker, Git repo picker, group/new-tab/rename/delete modal drafts, icon picker state, and any other modal draft or selection;
- computed geometry and render-only hit areas currently represented by `ViewState`;
- terminal scrollback viewport offsets when those are view choices rather than shared document data.

Client surface state stays per connection, not in shared session state: viewport size, cell size, negotiated render encoding, render baseline, render pending flag, Kitty graphics cache, host mouse-capture mode, staged clipboard-image files, and writer channels. A watcher’s viewport is presentation state: it crops or pads the controller-sized canonical canvas and does not become a second PTY size. Much of this already lives on `ClientConnection`; the normal app view/navigation state is the separate client-view seam.

## Construction and reconciliation

A new app client starts with a default view cloned from the current shared/default view when one exists, otherwise from a deterministic projection of shared session state and config. Restore and handoff persist shared session data plus one default view, not every transient attached client view or Tab Control assignment. Old snapshots migrate into that default view so users restore the same visible workspace, tab, and pane they had before the split.

Structural mutations stay shared. Creating, deleting, moving, or renaming workspaces/tabs/panes updates the shared session and then reconciles every live client view to valid ids. Reconciliation should prefer stable public/internal ids over raw indices; indices are acceptable only as cached projections rebuilt from shared state. If a viewed object disappears, the client falls back to the nearest valid sibling, then the active/default workspace, then the built-in empty state.

## Input, render, and API rules

ADR 0010 still applies: normal thin clients forward bytes and the server remains the semantic input owner. The input route must include the source client so navigation/view actions mutate that client's view, while shared object operations mutate the shared session. Interactive tab input and canonical resizing are accepted only from that tab's controller; a watcher must explicitly take over first. Direct terminal attach input still bypasses normal app semantic routing.

ADR 0002 still applies: render remains read-only. View computation may reconcile client-local geometry and scroll bounds before drawing; drawing takes immutable shared state plus immutable client view state. Background client renders and watcher resize/focus/input must not resize shared PTYs or change terminal content. Under ADR 0056, only the tab controller can change canonical runtime size; the foreground client supplies host context but does not replace Tab Control.

ADR 0013 still applies: the Local API is the control plane. API calls with explicit workspace/tab/pane ids are canonical, bypass interactive Tab Control, and do not depend on a client view. System automation follows the same boundary. Legacy no-target API/plugin operations use the default view only as a compatibility fallback and must not claim a tab controller; plugin event context should carry the invoking client/view when a plugin action is view-sensitive.

Notifications remain centralized unless a later ADR changes delivery policy. View-sensitive suppression and toast click handling must be evaluated against the relevant client view or foreground host context, and a toast click in one client must not navigate another client.

## Terminal viewport split

Terminal runtime output, parser state, history buffers, and pane terminal identity stay shared runtime/session data under ADR 0001. A client's scrollback viewport, search, and navigation position are client view state; a watcher keeps those choices local while observing the controller-sized canvas.

## Current boundary

Normal app behavior is divided into shared session state, client view state, client surface state, and Tab Control/runtime coordination. Structural mutations reconcile every live client view to valid stable ids. Tab control is transient per stable tab identity: navigation away and disconnect release the controller, and no watcher is automatically promoted. Persistence, restore, handoff, notifications, API/plugin fallbacks, and direct attach behavior must preserve these boundaries.

The split should not introduce a wire-protocol bump until a client-visible protocol field changes. Internal server routing can carry client ids, view state, and Tab Control state without changing framed bytes. If later work persists multiple named client views or exposes view ids over the API, that will need its own compatibility review.

## Current rationale

`[INFERENCE]` Users expect multiple attached Gardn app clients to behave like multiple views into one workspace manager, not a screen share where every navigation action yanks other clients around. Explicit Tab Control preserves that independence while ensuring only a deliberate controller can alter a tab's canonical PTY size or interactive input; terminal/runtime ownership remains server-side.

## Consequences

New app features must choose an owner explicitly: shared session, client view, client surface, Tab Control, or terminal runtime. Fields that only describe what one attached client is looking at belong in client view or surface state. Fields that describe durable workspace objects, terminal identity, committed settings, API-visible state, or real runtime ownership belong in shared session state or runtime coordination. Per-tab size and interactive input must never be inferred from global foreground status.

Multi-client behavior should preserve user-visible independence: two clients can select different workspaces/tabs/panes, open different modals, scroll and search different terminal views, and render at different sizes without mutating each other's views. For a controlled tab, tests and QA should also assert that watcher actions do not change canonical size or content, explicit takeover does, and controller navigation or disconnect releases without auto-promotion. Shared behavior should converge for workspace structure, terminal output, committed settings, Git mutations, command runs, and agent state changes.

