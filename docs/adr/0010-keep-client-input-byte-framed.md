---
status: accepted
---

# Keep client input byte-framed and server-decoded

Hako thin clients read stdin bytes, pass them through `RawInputByteFramer`, and forward complete input chunks to the server as `ClientMessage::Input`. The framer preserves complete escape/control-string boundaries across stdin reads and may intentionally drop incomplete host-control replies after timeout so reply tails do not leak as pane input. For normal app clients, `HeadlessServer` is the semantic input owner: it parses each framed message into `RawInputEvent`s, updates server-side per-client focus/theme/render-redraw state, promotes active foreground clients, applies the foreground client's keybinding profile, and routes events through `App::route_client_events` and the app input handlers.

This is accepted because the server owns shared app state, terminal runtimes, active client arbitration, keybinding application, and pane routing. Duplicating semantic key/mouse/paste parsing in every client would make remote behavior diverge from the server and from monolithic mode. Keeping the client mostly byte-framed preserves complete terminal escape/control sequences for server interpretation in the current foreground-client/app-mode context, while allowing the client framer to drop incomplete host-control replies that would otherwise leak.

## Considered options

- Decode key, mouse, paste, focus, and host-theme events fully in the client and send semantic events to the server. Rejected because it would duplicate app input logic and make client/server keybinding, prefix, mouse, paste, and focus behavior drift.
- Send unframed stdin chunks directly to the server. Rejected because terminal control strings can be split across reads, and `HeadlessServer` currently parses each `ClientMessage::Input` independently rather than carrying parser state per client.
- Keep clients byte-framed, with narrow local exceptions for client-only behavior, and let the server decode normal app input. Accepted because it preserves a single semantic input owner while still letting clients handle direct-attach detach escape, local clipboard image bridging, and the outer-focus redraw hint needed for their own host terminal surface.

## Consequences

New client input behavior should avoid moving app semantic routing into `apps/hako/src/client`. The client may buffer/frame raw bytes, filter direct-attach detach escape sequences, detect local clipboard image paste triggers, and parse enough host-surface events to know whether its own terminal surface needs a redraw. It should still forward normal app input as `ClientMessage::Input` bytes.

The server must remain the owner of app input semantics. Normal app clients are decoded in `HeadlessServer`, where client activity can promote foreground ownership, client-local keybindings can be applied, host terminal theme/focus can update server state, and `App::route_client_events` can handle key, mouse, paste, focus, and host-theme events. Direct terminal attach is intentionally different: once `AttachTerminal` has established ownership and terminal-attach mode, `ClientInput` bytes from that connection bypass app semantic decoding and are forwarded directly to the target terminal runtime.

Historical rationale beyond the current source is `[INFERENCE]`: this boundary likely exists so thin clients can stay small transport/render shims while one server process preserves consistent app behavior across local UI, remote clients, semantic rendering, terminal-ANSI rendering, and direct terminal attach.
