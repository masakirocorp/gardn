---
status: accepted
---

# Cap protocol payloads and shed oversized visuals

Hako caps wire and thin-client transport payloads at defined stages and treats oversized visual payloads as droppable. Inbound length-prefixed frames are rejected before payload allocation when the claimed frame length exceeds the allowed cap. Outbound control/render frames are size-checked before queueing or sending. Decoded client input and clipboard image messages also have payload-specific limits inside the framed transport cap.

This favors bounded resource use over preserving every visual artifact. Graphics are useful presentation data, but they are not the control channel; when image data exceeds a client's graphics frame budget, Hako can drop the image payload and retry the render update as text-only instead of blocking interactive delivery.

This is separate from ADR 0006's protocol versioning and ADR 0029's render stream negotiation. Those ADRs record compatibility, per-client render baselines, and fresh-frame preference; this ADR records the resource ceilings and visual-shedding policy that keep those streams safe under large payloads.

## Current rationale

`[INFERENCE]` Hako caps payloads because the wire protocol accepts length-prefixed data from client/server peers and must reject hostile or accidental oversized inbound frames before allocating buffers. It sheds oversized visuals because a degraded text-only frame is better than stalling an interactive terminal behind an image payload a client cannot reasonably accept.

## Consequences

New wire messages and client payload types need explicit size limits at the right layer: frame caps before inbound read allocation, send/queue caps before outbound delivery, and decoded payload limits for message bodies that can grow independently. New render features should have a bounded fallback path when their visual data can exceed normal frame budgets, and they should not make the control channel or text rendering depend on delivering every large visual payload.
