---
status: accepted
---

# Split local API from client wire protocol

A persistent/headless Hako server exposes two local Unix-domain socket transports. The local API socket accepts newline-delimited JSON `Request` values from `apps/hako/src/api/schema.rs` for status, server control, workspace/tab/pane/agent operations, waits, subscriptions, integrations, and capability discovery. The client socket accepts the binary wire protocol from `apps/hako/src/protocol/wire.rs` for interactive thin-client attach, render frames, negotiated keybinding/render modes, terminal attach, and byte-framed input.

Headless server mode starts both transports: `apps/hako/src/server/headless.rs::run_server` starts `api::start_server` for the JSON API, then `HeadlessServer::new` binds the client-protocol socket. Normal `hako` startup checks the client socket; if an existing server is listening, it validates compatibility through API `ping` status, otherwise it spawns a server and waits for the client socket before attaching. When `HAKO_SOCKET_PATH` is accepted as the active API socket override, the client socket is derived by inserting `-client` before `.sock`; explicit session selection overrides both, and inherited release/dev app-dir socket overrides can be ignored when they do not match the current binary's app dir.

The API is the control plane, not the render/input stream. API clients get typed request/response results, event subscriptions, output reads, and `ping` responses containing Hako version and wire protocol version, plus optional server capabilities; current headless servers report live handoff. Interactive clients use the separate wire protocol because rendering and input are high-volume ordered streams with protocol negotiation, frame-size limits, foreground-client arbitration, and direct-terminal-attach ownership rules that do not belong in the JSON command API.

## Current rationale

`[INFERENCE]` Keeping the API and client protocol separate lets automation use a stable, inspectable JSON surface without inheriting the latency, framing, and compatibility constraints of the interactive render/input stream. Keeping accepted socket overrides path-coupled preserves one server identity while allowing the transports to evolve independently.

## Consequences

CLI status/update/server/workspace/pane/agent/integration commands should use the local API client where possible instead of speaking the binary client protocol. Direct `agent attach`/terminal attach and thin-client render/input stay on the client wire protocol. Thin clients should not grow ad hoc JSON control messages for app mutations; those belong in `apps/hako/src/api/schema.rs` and dispatch through the app API request channel.

The API ping response intentionally reports the wire protocol version even though it is not itself the wire protocol. That lets clients and update paths detect whether a running server can accept the binary client they are about to attach or replace. API `pane.report_*` and integration calls are explicit reports; fallback screen detection remains terminal-tail inference owned by app/runtime state, not by the JSON API transport.

Socket path changes must preserve accepted API/client coupling. If `HAKO_SOCKET_PATH=/tmp/hako.sock` is accepted as the active API socket path, the client protocol socket is `/tmp/hako-client.sock`; explicit session selection overrides both. `HAKO_CLIENT_SOCKET_PATH` remains a legacy/client-only fallback when no API socket override is set.
