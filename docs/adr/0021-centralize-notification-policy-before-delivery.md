---
status: accepted
---

# Centralize notification policy before delivery

Hako treats agent state notifications as product events derived from effective agent state transitions before choosing a delivery channel. `notification_toast_for_state_change` and `notification_sound_for_state_change` encode the shared policy: no notification when state is unchanged; blocked agents produce needs-attention notifications and request sounds; background completion transitions to idle produce finished notifications and done sounds. Active-tab handling is channel-specific: Hako toasts are only built for background tabs, terminal/system client notifications are suppressed when the active tab is focused or focus reporting is unavailable, and request sounds can still fire for active-tab attention while done sounds remain background-only.

`ToastKind`, `ToastNotification`, `ToastTarget`, `PendingAgentNotification`, and `AgentNotificationDelivery` are the app-level notification vocabulary. State-change paths use shared policy helpers before delivery, and delayed delivery stores `PendingAgentNotification` by pane. When the delay elapses, Hako revalidates the expected agent state and effective agent label before producing `AgentNotificationDelivery`, so stale transitions do not notify after the pane has moved on. Hako toasts are in-app UI and can carry a `ToastTarget` so mouse clicks, prefix navigation, and command-palette actions can focus the affected workspace/tab/pane. Terminal and system notifications receive formatted title/body text and do not carry focus targets back into Hako.

`ToastDelivery` is the user-facing delivery switch: `off`, `hako`, `terminal`, or `system`. `hako` stores a `ToastNotification` in `AppState` and uses per-kind deadlines to clear it. `terminal` emits OSC notifications through detected terminal backends such as Ghostty, iTerm2, Kitty, or WezTerm; unsupported terminals return `Ok(false)`. `system` uses the platform desktop notification boundary. Both terminal and system delivery are best-effort edge effects; failures do not rewrite agent state or create fallback Hako toasts.

Headless server mode forwards delivery effects to the foreground client instead of performing every side effect inside the server. `server::notifications::toast_notify_kind` maps `terminal` to `NotifyKind::Toast` and `system` to `NotifyKind::SystemToast`; `off` and `hako` are not forwarded as client toast notifications. Sound forwarding is separate from toast forwarding and uses `NotifyKind::Sound`. Client code then invokes terminal/system notification APIs or sound playback locally, where foreground terminal and desktop context exist.

This is separate from ADR 0010's byte-framed client input and ADR 0013's local API transport. API state reports such as `pane.report_agent` feed the same effective-state policy as other agent state inputs. The separate `notification.show` API is an explicit notification command with its own sanitization, rate limits, busy/no-foreground responses, and direct delivery behavior.

## Current rationale

`[INFERENCE]` Hako centralizes notification policy because agent state can arrive through multiple paths such as fallback screen detection, hook/integration reports, and `pane.report_agent`. If each delivery path decided independently, active-tab handling, sound policy, notification text, and focus behavior would drift.

`[INFERENCE]` Hako keeps terminal/system delivery best-effort because those channels depend on host terminal, foreground client, desktop platform, and user environment support. Treating delivery failure as a state change would make agent state less reliable than the delivery backend.

## Consequences

New agent-state notification sources should use the shared policy helpers and app-level notification vocabulary (`ToastKind`, `ToastNotification`, `PendingAgentNotification`, `AgentNotificationDelivery`) before touching delivery-specific APIs. They should not call terminal/system notification functions directly unless they are already at a client/platform edge. Non-agent API notifications may keep their explicit `notification.show` contract, but should not bypass rate limiting or sanitization.

New delivery channels should preserve the same state-transition policy and channel-specific active-tab handling. If they cannot support `ToastTarget`, they should degrade like terminal/system delivery rather than weakening the in-app Hako toast target contract.

Headless-server notification work must route through the forwarding path so clipboard, sound, terminal toast, and system toast side effects happen in the foreground client context.
