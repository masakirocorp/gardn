---
status: accepted
---

# Keep local API events bounded and replayable

Oh My Herdr treats local API events as process-local recent history, not as durable audit logs or best-effort push-only messages. App code emits `EventEnvelope` values through `App::emit_event`, which appends them to the shared `EventHub`. Each pushed event receives an internal monotonically increasing sequence number, and the hub retains only the most recent 512 events.

`events.subscribe` is a streaming request. The API server validates and constructs all requested subscriptions first; if any subscription is invalid, it writes the error response and closes the request. If construction succeeds, the server writes a `SubscriptionStarted` response before entering the poll loop. That ack is the boundary after which clients can treat subsequent newline-delimited values as subscription events.

Event subscriptions keep their own in-memory cursor. Each `ActiveEventSubscription` starts with `last_sequence = 0`, polls `EventHub::events_after(last_sequence)`, advances across retained events, and emits the first matching event kind as JSON. Non-matching retained events still advance the cursor. New subscribers therefore can replay retained matching events that are still inside the process-local 512-event buffer, but they cannot ask the server to resume from a client-provided sequence or recover events already evicted from memory.

The event hub is intentionally narrower than all subscription behavior. EventHub-backed subscriptions replay app-emitted `EventEnvelope` values from the bounded hub only when a `Subscription` variant maps to an `ActiveEventSubscription`. Other streaming subscriptions such as pane output matches and pane agent status changes poll current pane state through API requests and emit `SubscriptionEventEnvelope` values; those subscription outputs are not stored in `EventHub` and do not use the 512-event retention policy. App code may still emit similarly named `EventEnvelope` values, such as `PaneAgentStatusChanged`, without exposing them through an EventHub-backed subscription.

This is separate from ADR 0013's local API transport decision. ADR 0013 says the JSON local API is the control plane and is separate from the thin-client wire protocol. This ADR records the local API event-history contract inside that control plane: sequence-numbered, in-memory, bounded, and replayable only while retained.

## Current rationale

`[INFERENCE]` Oh My Herdr keeps event history bounded because events are operational signals for local automation, not a durable source of truth. A fixed in-memory buffer avoids unbounded growth in long-running servers while still smoothing over short subscriber startup races.

`[INFERENCE]` The `SubscriptionStarted` ack gives clients a clean stream boundary: response parsing finishes before event parsing begins. Replaying retained events after that boundary reduces missed workspace/tab/pane events during connection setup without introducing durable offsets or persistence.

`[INFERENCE]` Keeping output/status subscriptions outside `EventHub` avoids pretending that all interesting automation conditions are app-emitted events. Output matching and agent status subscriptions depend on current pane contents and presentation state, so polling their Local API seams keeps them aligned with what clients can observe. The schema-level `events.wait` response shape is outside this replay contract unless it is implemented through the event hub in a later change.

## Consequences

New app event kinds should be emitted as `EventEnvelope` values through `App::emit_event` if they belong to local API event history. To become replayable via `events.subscribe`, they also need a `Subscription` variant that maps to `ActiveEventSubscription`; emission alone is not enough.

New subscriptions must choose whether they are event-history subscriptions or state-polling subscriptions. Event-history subscriptions should use `EventHub` and its bounded sequence cursor. State-polling subscriptions should not claim event replay guarantees unless they also emit, retain, and expose matching `EventEnvelope` values through EventHub-backed subscriptions.

Clients must treat local API event replay as opportunistic recent history. They should not depend on persistence across server restarts, exact retention beyond 512 events, or resuming from client-owned offsets.
