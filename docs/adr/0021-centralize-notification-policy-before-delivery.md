---
status: accepted
---

# Centralize notification policy before delivery

Gardn produces each state notification once from an accepted effective state transition or delayed expiry. A coordinator owns the notification epoch, monotonic IDs, presenter registrations, selection, dispatch, expiry, and receipt state. AppState stores only the pure Gardn toast value. Render remains pure.

`StateNotification` is the shared data shape. It carries a stable `NotificationId`, source, optional workspace/tab/terminal target, title, body, visual mode, sound, creation time, and expiry time. Presenter registrations advertise a rendering host and capabilities. A `PresentationRequest` bundles visual and sound effects when one presenter can perform both. A receipt reports `submitted`, `rejected`, or `unknown`. Submitted means the platform accepted the request or the terminal write completed. It does not mean that a person saw it.

The coordinator selects at most one presenter per destination. `off` suppresses delivery. `gardn` stores one in-app toast and does not send a duplicate visual request. `terminal` selects the foreground terminal capability. `system` prefers the oldest live native Local API system presenter, then a foreground thin-client system presenter, then a foreground thin-client terminal presenter. For Gardn plus sound, Gardn stores the visual toast once and the coordinator sends sound-only work to one selected host presenter.

Presenter streams use typed JSON-lines Local API requests. Registration becomes eligible only after its acknowledgement is written. Closing a stream unregisters it. Binary clients register capabilities on their actual connection and return typed receipts. Receipts are accepted only for the owning registration and pending notification. Separate Local API receipt connections correlate by registration and notification IDs. A stream close retires the registration and pending outcomes become unknown.

The coordinator keeps bounded outstanding work and deduplicates by notification and registration IDs. Expired work is never dispatched. Fallback is allowed only before a request queue accepts ownership. Rejection, timeout, disconnect, and ambiguous writes are terminal and never replay on reconnect. EventHub remains observational and is not the presenter queue.

The explicit `notification.show` method remains a distinct producer. Its terminal, system, and sound effects enter the same coordinator runtime and its result reports queued or suppressed semantics. It does not claim that enqueue made a notification visible.

## Consequences

New state paths must report effective transitions to the single producer. They must not infer transitions again in headless or API code, call platform notification functions directly, or decode sound strings. Native and thin clients are transport presenters. A future mobile integration may implement a narrow authenticated outbound delivery seam without exposing Local API types or adding a provider, credentials, or durable queue.
