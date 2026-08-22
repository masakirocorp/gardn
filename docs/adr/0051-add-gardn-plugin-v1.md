# 0051. Add Gardn plugin v1

## Status

Accepted

## Context

Gardn is adding a plugin system. Plugins are local extension code that can add actions, panes, link handlers, and event hooks through Gardn's local API and CLI surfaces.

This is a public trust boundary. A plugin runs as the user, can execute commands, and can receive Gardn runtime context through environment variables. Gardn must not present plugin discovery or installation as sandboxing or endorsement.

`ogulcancelik/herdr` introduced the plugin v1 behavior that Gardn ports. Gardn keeps its own public names and storage semantics.

## Decision

Gardn will support plugin v1 with these rules:

- The manifest is `gardn-plugin.toml` with `min_gardn_version`. Gardn does not accept compatibility aliases for either name.
- Plugin installs are global to the Gardn app, not per session.
- Plugin runtime logs and pane attribution are session-local runtime state.
- Plugins run unsandboxed as the current user.
- Remote installs must preview source/build/action/pane/link/event metadata and require confirmation unless `--yes` is passed.
- Plugin command execution receives documented `GARDN_*` context and protected Gardn/plugin environment keys cannot be overwritten by plugin env overrides.
- Plugin panes are normal Gardn terminal panes. `AppState` stores attribution metadata; `TerminalRuntimeRegistry` owns live terminal runtimes.
- Plugin pane attribution must follow pane moves and be removed when panes/tabs/workspaces are removed or when a plugin is unlinked.
- The local JSON API is the plugin control plane. Plugins do not use the client render/input wire protocol.
- Windows plugin support is best-effort in v1: path and command resolution should be Windows-aware, but raw socket and process-lifecycle parity is not promised beyond tested behavior.

## Consequences

Gardn gains an extension platform without treating plugins as agent profiles.

The plugin API is a compatibility surface. Future changes to manifest fields, env vars, storage roots, and plugin API methods require migration or deliberate breakage.

Users must treat plugins like shell scripts they install and run locally. Gardn should keep trust language explicit in CLI/docs and avoid implying marketplace review.
