---
status: accepted
---

# Negotiate per-client render encoding and prefer fresh frames

Gardn treats server-to-client rendering as a per-client stream, not as one global frame queue. Each thin client requests a `RenderEncoding` during the wire-protocol `Hello`; the server echoes the requested/selected encoding in `Welcome` and stores a separate `ClientRenderState` for that connection. Normal app clients default to semantic `FrameData`, while direct terminal attach clients request `TerminalAnsi` so they receive terminal byte streams instead of the full app frame model.

Semantic clients cache the last `FrameData` and skip identical frames. Terminal-ANSI clients keep a per-client blit encoder and sequence counter; they skip frames when the encoder is already current, and they commit the encoder plus sequence only after the frame is actually queued for that client. Client-side semantic receivers then encode `FrameData` into terminal bytes locally, while Terminal-ANSI receivers write the server-provided bytes directly.

Render delivery is intentionally freshness-biased. Client writers have a non-droppable prioritized control channel for shutdown, notifications, clipboard writes, and other control messages, plus a separate capacity-one render channel. `render_and_stream` uses `try_send` for render frames: if a client's render slot is full, Gardn marks that client `render_pending` and keeps the server loop moving instead of buffering old frames. When the writer drains the slot, the server event loop can render again and send the latest state.

Oversized graphics follow the same freshness rule. If a frame's graphics bytes exceed the graphics frame limit, Gardn drops graphics for that client frame and does not commit the graphics cache. If serialization is oversized only because of graphics, Gardn retries as text-only. A naturally oversized no-graphics frame is skipped without committing the render baseline. If the text-only retry itself cannot be serialized, Gardn treats that client as broken rather than blocking the server or corrupting the client's render baseline.

This is separate from ADR 0011's direct terminal attach ownership: direct attach exclusivity decides who can write to one terminal runtime, while this ADR records how that connection receives render output. It is separate from ADR 0013's Local API/wire split: render frames are interactive wire-protocol messages, not Local API events. It is also separate from ADR 0027's Host Graphics cache: per-client render state decides whether and when a frame is sent; host graphics reconciliation decides which Kitty graphics bytes belong inside that frame for a given client surface.

## Current rationale

`[INFERENCE]` Gardn keeps render state per client because attached clients can have different sizes, encodings, host graphics state, and latency. A reliable replay queue would preserve old frames at the cost of making slow clients lag behind interactive state; a freshness-biased one-slot render channel keeps the server responsive and lets slow clients converge on the newest frame when they drain.

## Consequences

New render encodings should add explicit `RenderEncoding` negotiation and their own per-client baseline state. They should not reuse another client's render cache or assume all clients share size, graphics, or frame history.

New server-to-client messages should choose the control channel for non-droppable operational effects and the render channel for droppable visual updates. Visual updates should be safe to skip when a newer frame can replace them.
