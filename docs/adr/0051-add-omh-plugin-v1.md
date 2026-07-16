# 0051. Add Oh My Herdr plugin v1

## Status

Accepted

## Context

Oh My Herdr is adding a plugin system. Plugins are local extension code that can add actions, panes, link handlers, and event hooks through Oh My Herdr's local API and CLI surfaces.

This is a public trust boundary. A plugin runs as the user, can execute commands, and can receive Oh My Herdr runtime context through environment variables. Oh My Herdr must not present plugin discovery or installation as sandboxing or endorsement.

Upstream Herdr added a plugin v1 system. Oh My Herdr should port the behavior, but keep Oh My Herdr product names and storage semantics.

## Decision

Oh My Herdr will support plugin v1 with these rules:

- The preferred manifest is `omh-plugin.toml` with `min_omh_version`.
- For upstream ecosystem compatibility, Oh My Herdr also accepts `herdr-plugin.toml` and `min_herdr_version` as aliases.
- Plugin installs are global to the Oh My Herdr app, not per session.
- Plugin runtime logs and pane attribution are session-local runtime state.
- Plugins run unsandboxed as the current user.
- Remote installs must preview source/build/action/pane/link/event metadata and require confirmation unless `--yes` is passed.
- Plugin command execution receives documented `OMH_*` context and protected Oh My Herdr/plugin environment keys cannot be overwritten by plugin env overrides.
- Plugin panes are normal Oh My Herdr terminal panes. `AppState` stores attribution metadata; `TerminalRuntimeRegistry` owns live terminal runtimes.
- Plugin pane attribution must follow pane moves and be removed when panes/tabs/workspaces are removed or when a plugin is unlinked.
- The local JSON API is the plugin control plane. Plugins do not use the client render/input wire protocol.
- Windows plugin support is best-effort in v1: path and command resolution should be Windows-aware, but raw socket and process-lifecycle parity is not promised beyond tested behavior.

## Consequences

Oh My Herdr gains an extension platform without treating plugins as agent profiles.

The plugin API is a compatibility surface. Future changes to manifest fields, env vars, storage roots, and plugin API methods require migration or deliberate breakage.

Users must treat plugins like shell scripts they install and run locally. Oh My Herdr should keep trust language explicit in CLI/docs and avoid implying marketplace review.
