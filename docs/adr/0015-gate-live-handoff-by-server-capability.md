---
status: accepted
---

# Gate live handoff by server capability

Hako treats live handoff as an opt-in self-update policy that only runs against servers that advertise handoff support. `hako update --handoff` still uses the GitHub Release/direct-install path from ADR 0012. During `self_update`, Hako reads running target status and decides per target before downloading; after installing the new binary, it sends `server.live_handoff` only to targets whose API `ping` response included `ServerCapabilities { live_handoff: true }`.

The policy lives in `src/update.rs`: `confirm_running_server_update_action` selects live handoff for capable servers, prompts before scheduling a stop for incapable servers that require restart, and otherwise leaves old servers running. Plain `hako update` does not hand off; it downloads first, prompts before installing and stopping running servers, installs only after confirmation, and asks the user to retry later when stopping is declined.

The mechanism is headless-server only. `api::start_server` advertises live handoff for headless servers, `ServerLiveHandoffParams` carries import executable/version/protocol expectations, `hako server live-handoff` calls `server.live_handoff`, and app-mode API handling rejects that method as `unsupported_in_app_mode`. `HeadlessServer::perform_live_handoff` disconnects clients, rejects pending client accepts, pauses handoff readers, captures a handoff snapshot, spawns/imports the replacement server, transfers PTY file descriptors, removes public sockets after the replacement has restored imports so it can bind them, restores old sockets on pre-commit ready/commit failure, then commits ownership and exits after the replacement acknowledges ownership.

This ADR is about update policy and capability gating. ADR 0009 records what data belongs in handoff snapshots versus durable session snapshots/history. ADR 0012 records how the updated binary is sourced. This decision records when Hako is allowed to use handoff during update and what happens when it cannot.

## Current rationale

`[INFERENCE]` Live handoff can preserve shells, agents, dev servers, and tests during an update, but it crosses process, socket, PTY, protocol, and app-state ownership boundaries. Gating it on an advertised capability avoids asking old or monolithic servers to perform a protocol they may not implement. Keeping it opt-in avoids surprising users with a complex server replacement path when a normal stop/restart update is acceptable.

## Consequences

A handoff-capable running server may be replaced without killing pane processes. An incapable server that must restart is never silently killed: under `--handoff`, interactive updates ask whether to stop it after the new binary is installed; declined prompts install the new binary but keep the old server running with guidance, while non-interactive updates abort before install with guidance.

If handoff fails after the binary is installed, Hako classifies the post-failure server state. If the updated server is already running, the update is treated as complete. If the old server is still running, Hako asks whether to stop it or keeps it alive; non-interactive updates keep it alive. If no server responds, Hako prints session attach/restart guidance. If state cannot be determined, Hako prints reconnect/stop guidance instead of pretending the live transition succeeded.
