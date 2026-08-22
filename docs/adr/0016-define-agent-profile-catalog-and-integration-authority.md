---
status: accepted
---

# Define agent profile catalog and integration authority

Gardn uses a single global agent profile catalog for launch choices. `apps/gardn/src/agent_profiles.rs` builds the catalog by layering read-only system profiles for every supported `AgentKind::SYSTEM` family with user profiles persisted under `[agent_profiles]`; the same `AgentKind` set is derived from socket API `IntegrationTarget` values. `AgentProfilesConfig.order` is the global order. Groups store only `favorite_agent_profile_ids` and `default_agent_profile_id`, so group settings can promote or default profiles without redefining commands, environment, kind, or order.

Profile launches use the catalog but do not create a separate profile runtime model. `New Agent` resolves a profile id, requires `available()` (`enabled`, parseable, and non-empty parsed argv), executes the raw configured command through the configured pane shell context with profile environment applied, names the tab from the profile, and records parsed `launch_argv` plus `launch_env` on the terminal for restore/session planning. The recorded `launch_argv` is not the exec form used by profile launch. The public `agent.start` API remains lower-level: it accepts explicit `argv` plus optional placement/focus/split, has no profile id or profile env, bypasses the catalog, and creates an argv-backed split in the requested or active placement, or a new workspace only when none exists.

Before spawning a supported profile, Gardn removes any profile-supplied `GARDN_AGENT` value and injects the canonical `GARDN_AGENT=<kind>` identity derived from the selected profile. The terminal records this managed launch environment, so wrapped processes inherit the same identity during launch and later restore planning. Profiles with `kind = "custom"` do not receive an agent identity hint.

Integration authority is separate from launch profiles. `pane.report_agent` is rejected for the reserved native state sources `gardn:claude`/`claude` and `gardn:codex`/`codex`; other sources may report state subject to terminal-state arbitration. Current session-only installed hooks such as Claude, Codex, Kimi, Droid, Cursor, OpenCode, and Hermes should use `pane.report_agent_session` for session identity while fallback screen detection owns state. Hermes is identity-only: its plugin never reports lifecycle state. OpenCode TUI selection reports `session_start_source=select` from a separate TUI plugin. `pane.report_agent` and `pane.report_agent_session` can provide official session refs and filtered launch-env values for restore, but reported environment only seeds missing launch context or accompanies an accepted session replacement; it cannot rewrite established context for the same session. `pane.release_agent` can provide an official session ref for matching/releasing authority, but not launch env. `pane.report_metadata` can add title/display/custom-status/state-label presentation, optionally scoped with `applies_to_source`, independently of launch context.

## Current rationale

`[INFERENCE]` Gardn keeps profiles global because agent commands are reusable launch identities, not workspace policy. Group favorites/defaults are presentation and workflow preferences, so they should not fork the catalog or create per-group command copies. Gardn keeps integration authority separate because a command profile says how to start an agent, while socket reports say what a running pane currently is and which source is allowed to claim state, session identity, or metadata.

## Consequences

Adding a new built-in agent family must update the shared family/target mapping, system profile defaults, integration install/status behavior, and detection/restore support as appropriate. `[INFERENCE]` A user wrapper should normally be a user agent profile with a known kind when native restore should keep working, or `custom` when Gardn should treat it as launch-only.

Deleting a user profile removes it from `[agent_profiles]`, the global order, group favorites, and group defaults. Reordering profiles changes global display order everywhere; groups can only split that global order into favorite and available sections.

State-reporting integrations must not use reserved native session-identity sources to override fallback screen-detected state. Session restore should trust only official sources accepted by `agent_resume`, and should keep only supported launch-env variables from reports.
