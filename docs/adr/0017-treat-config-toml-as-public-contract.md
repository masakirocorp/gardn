---
status: accepted
---

# Treat config.toml as a public contract

Oh My Herdr treats `config.toml` as a user-editable product API, not only as serialized app state. `apps/omh/src/config/model.rs::Config` exposes a top-level `onboarding` key plus stable sections for theme, terminal, session, keys, UI, advanced, worktrees, experimental, remote, and agent profiles. `apps/omh/src/config/io.rs::config_path` resolves the file from `OMH_CONFIG_PATH` or the Oh My Herdr config dir (`$XDG_CONFIG_HOME/<app>/config.toml`, `$HOME/.config/<app>/config.toml`, then `/tmp/<app>/config.toml`; `<app>` is `omh` in release and `omh-dev` in debug). Compatibility aliases stay in the reader when old field names remain meaningful: examples include keybinding `fullscreen` for `zoom`, theme `name` beside light/dark/mode, toast `enabled` beside `delivery`, and advanced `scrollback_lines` beside `scrollback_limit_bytes`.

Settings writes preserve unrelated hand-edited file content instead of regenerating the whole document. `apps/omh/src/app/config_io.rs::update_config_file` reads the current file, creates it if needed, and uses simple text upserts/removals for each helper's owned top-level key, section key, or owned section. Rewritten key lines and rewritten owned sections are not comment-preserving: theme save removes legacy `theme.name`, toast save removes legacy `ui.toast.enabled`, and agent profile saves rewrite the `[agent_profiles]` section. The main Settings save can call multiple helpers; successful helper writes call `apply_config_from_disk(false)` so the app converges on the same typed config that a manual edit plus reload would produce.

Live reload is section-scoped and diagnostic-first for the current live sections. `apps/omh/src/config/io.rs::load_live_config` parses the whole file as TOML first; read or parse failure keeps current runtime config and returns `Failed`. It then deserializes the current live sections independently (`theme`, `keys`, `terminal`, `session`, `ui`, `advanced`, `worktrees`, `experimental`, and `agent_profiles`), records invalid section names, and returns diagnostics that say which settings were kept. `App::apply_live_config` applies valid sections onto current runtime state and skips invalid sections rather than replacing the whole app config all-or-nothing. `[remote]` is public config but is not currently live-applied, and top-level `onboarding` is parsed on live reload without changing the current UI mode.

`server.reload_config`, prefix reload, and headless reload surface `ConfigReloadReport` semantics; the API serializes `ResponseResult::ConfigReload { status, diagnostics }`. A complete file parse/read failure is `Failed`, while section errors, keybinding validation errors, or validation warnings are `Partial`.

Unsafe UI values have explicit startup and reload behavior. `validated_sidebar_bounds` rejects `sidebar_min_width > sidebar_max_width` before any `u16::clamp` can panic. At startup, invalid bounds fall back to built-in defaults so Oh My Herdr can render. During live reload, invalid sidebar bounds make `[ui]` keep the previous live UI settings and produce a warning instead of partially applying an unsafe UI section.

## Current rationale

`[INFERENCE]` Oh My Herdr keeps the config file stable and hand-editable because agents and users need to change it across sessions without going through the TUI. It prefers compatibility aliases over migration churn because old config keys can be harmlessly interpreted at the boundary. It prefers partial reload over all-or-nothing reload because one bad section should not discard unrelated valid changes or break a running server/client session.

## Consequences

New persistent settings should be added as typed config fields under a stable section, with compatibility aliases when renaming an existing public key. Save paths should rewrite the narrowest section or key they own and then reload through `apply_config_from_disk` instead of mutating runtime state through a parallel path.

Live reload code must decide section ownership explicitly. If a value can make later UI/runtime code panic or violate invariants, validate it before applying the section and keep the previous runtime state on failure. Diagnostics are part of the user/API contract; reload callers should surface them rather than silently falling back.
