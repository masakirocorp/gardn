---
status: accepted
---

# Make direct terminal attach exclusive by default

Oh My Herdr direct terminal attach gives one client writable ownership of one terminal unless another client explicitly requests takeover. The built-in direct-attach client requests `ClientLaunchMode::TerminalAttach` and `RenderEncoding::TerminalAnsi` in `Hello`, then sends `ClientMessage::AttachTerminal { terminal_id, takeover }`.

After that attach request succeeds, the server switches the connection into `ClientConnectionMode::TerminalAttach` and renders that one terminal runtime for the connection. The server uses `terminal_attach_owners` as the attach admission/takeover map for terminal-id strings.

After a successful attach, input forwarding is driven by the connection's `ClientConnectionMode::TerminalAttach`: `ClientInput` bytes are written directly to the target runtime, and `direct_attach_resize_locks` prevents normal app rendering from resizing that runtime while the attach owner is present.

This is accepted because direct attach bypasses normal app input semantics and writes raw bytes to a live terminal runtime. Multiple simultaneous writers to the same terminal would make keystrokes, paste, size negotiation, and detach behavior ambiguous. Exclusive ownership gives a predictable default while explicit takeover still lets a user recover or intentionally replace a stale owner.

## Considered options

- Allow multiple direct attach clients to write to the same terminal. Rejected because terminal input streams are ordered byte streams and multi-writer interleaving would be hard to reason about or debug.
- Reject all later attach attempts until the current owner disconnects. Rejected because a stale or inaccessible owner could trap a terminal.
- Use exclusive ownership with explicit takeover. Accepted because it protects the normal case and still provides an intentional recovery path.

## Consequences

A direct attach request for a missing terminal shuts down that client with an explanatory error. A direct attach request for an already-owned terminal is rejected unless `takeover` is true. When takeover is true, the previous owner receives a shutdown message and is removed before the new owner is recorded. Once a client is in terminal-attach mode, subsequent input from that connection bypasses app semantic decoding and is forwarded directly to the attached terminal runtime.

Pending attach clients are excluded from normal app-client foreground/render accounting. After attach, they are terminal-attach render targets for one runtime. The built-in attach client uses terminal-ANSI rendering, keeps a client-local escape state, and detaches with the `Ctrl+B` then `q` sequence; doubled prefix sends a literal prefix and prefix followed by another byte forwards both. If the attached terminal dies, Oh My Herdr shuts down matching attach clients. When an attach client disconnects, Oh My Herdr clears its owner entry and releases the direct-attach resize lock; if a foreground app client remains, the shared render path recomputes unlocked runtime sizing from that app client's effective size.

Historical rationale beyond the current source is `[INFERENCE]`: this policy likely exists to make direct attach feel like controlling a real terminal device while avoiding accidental multi-client input races in remote/headless operation.
