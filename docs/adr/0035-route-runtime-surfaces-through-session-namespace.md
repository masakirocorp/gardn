---
status: accepted
---

# Route runtime surfaces through the session namespace

Gardn treats the active session name as the namespace for runtime-adjacent data surfaces. Parsing `--session` or `gardn session attach <name>` happens before normal command dispatch, and the chosen session re-roots the data directory, Local API socket, client socket, logs, handoff import socket, and status reporting.

This is broader than a CLI selector. The default session keeps the historical app directory layout, while named sessions live under `sessions/<name>` and get their own sockets and persisted runtime data. An explicit session request also takes precedence over inherited socket environment where needed, so commands target the requested session instead of accidentally attaching to another running server. Update discovery follows the same rule when a session is explicit; otherwise it can scan known sessions or follow the inherited socket target.

This is separate from ADR 0013's Local API / wire protocol split and ADR 0009's snapshot/history/handoff persistence split. Those ADRs define surface types and persisted content; this ADR records the namespace authority that decides which instance's paths those surfaces use.

## Current rationale

`[INFERENCE]` Gardn routes these surfaces through one session namespace so multiple server instances can coexist without sharing sockets, logs, handoff state, or persisted sessions. Keeping session selection process-global and early also makes CLI, server, updater, and status code converge on the same paths instead of each carrying unrelated selector logic.

## Consequences

New runtime-adjacent paths should derive from `session::data_dir_for`, `api_socket_path_for`, or `client_socket_path_for` rather than constructing app-global paths directly. New CLI/server flows that accept session selection must apply it before looking up sockets, logs, persistence, or handoff state. Update flows should scope to the explicit session when one is requested and otherwise keep their multi-session discovery behavior.
