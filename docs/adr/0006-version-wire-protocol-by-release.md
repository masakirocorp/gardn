---
status: accepted
---

# Version the wire protocol by release

Hako uses a single explicit `PROTOCOL_VERSION` for the server/client wire protocol and currently accepts only exact protocol matches during client handshake. `ClientMessage::Hello` sends the client protocol, `ServerMessage::Welcome` reports the server protocol, and `check_client_version` rejects pre-persistence, older, and newer clients instead of attempting backward or forward compatibility.

This records the repository policy that Hako's compatibility boundary is tagged Hako releases, not arbitrary intermediate `master` revisions. A wire-protocol change that makes current source incompatible with the latest Hako release protocol should bump `PROTOCOL_VERSION` once for that release cycle. Multiple unreleased incompatible wire changes before the next Hako tag share the same bump instead of incrementing for each commit.

## Considered options

- Accept older and newer protocol versions with compatibility shims. Rejected for now because the wire protocol has no negotiated feature set or compatibility matrix, and mismatched clients can misinterpret binary frames or semantic messages.
- Bump the protocol version for every unreleased wire change. Rejected because protocol compatibility is reviewed against tagged Hako releases, not every intermediate `master` revision.
- Compare current source against the latest Hako release tag and bump only when source is not already greater than the released protocol. Accepted because it makes release compatibility review explicit while avoiding noisy pre-release version churn.

## Consequences

When changing `src/protocol/wire.rs`, presentation encoding under `src/protocol/`, framing, handshake fields, frame-size limits, or server/client behavior that would make an older Hako release client or server misdecode, reject, or semantically misinterpret the stream, compare current source `src/protocol/wire.rs::PROTOCOL_VERSION` (re-exported as `crate::protocol::PROTOCOL_VERSION`) against the latest Hako release tag. If current source is not already greater than the latest released protocol, bump it once and update all hardcoded expectations and manual protocol fixtures that intentionally pin the value.

Tests and status surfaces intentionally expose protocol changes. Hardcoded protocol expectations in API ping, CLI status, client-mode, headless-server, detach/reattach, multi-client, and cross-area tests should be updated deliberately instead of following the constant automatically. API ping/runtime status, CLI status, server autodetect, remote install/bridge/live-handoff, handoff `expected_protocol`, client `Hello`, and server client-transport `Welcome`/version checks should continue reporting or enforcing the explicit protocol value.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this policy likely exists because Hako can ask users to restart or update tagged binaries, while maintaining compatibility across arbitrary unreleased client/server pairs would add negotiation complexity without a current product requirement.
