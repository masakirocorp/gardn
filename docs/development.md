# Local development

This file is maintainer and contributor tooling. It is not the public product manual.

## Just recipes

Just owns imperative workflows. Routine test, lint, and check tasks stay in `pnpm` / Turbo.

Run `just --list` for the live index. The Justfile comments are the source of truth.

**Local binaries**

| Recipe | What it does |
| --- | --- |
| `just install-local` | Build and install `gardn-dev`. Does not replace production `gardn`. |
| `just install-dev` | Alias for `install-local`. |
| `just copy-session-to-dev` | Copy `~/.config/gardn` session state into `~/.config/gardn-dev`. |

**Upstream and release**

| Recipe | What it does |
| --- | --- |
| `just sync-upstream` | Create a merge-commit PR from the upstream repository. |
| `just upstream-status` | Report upstream commits as ported, skipped, superseded, or pending. |
| `just release` | Draft a Tegami version commit, tag it, push, and trigger GitHub Release. |
| `just release-beta` | Same as `just release`, with Tegami prerelease id `beta`. |


**Checks and benches**

| Recipe | What it does |
| --- | --- |
| `just ui-hot-path-architecture-test` | Enforce UI hot-path architecture boundaries. |
| `just bench-render-scale` | Non-gating full-render scaling profile. |
| `just default-config` | Print the default config. |
| `just build-libghostty-vt` | Build the vendored libghostty-vt source dist. |

**Live agent tests**

| Recipe | What it does |
| --- | --- |
| `just agent-test-image` | Build the live agent test image. |
| `just agent-test-doctor` | Print versions from that image. |
| `just agent-test-verify` | Verify live agent test environment wiring without providers. |
| `just agent-test-opencode` | Run OpenCode against the configured free OpenRouter model. |
| `just agent-test-opencode-status` | Run OpenCode and verify Gardn status from the real plugin. |
| `just agent-test-pi-omp-status` | Run Pi/OMP and verify Gardn status from the real plugin. |
| `just agent-test-pi-omp-plugin-status` | Verify Pi/OMP plugin lifecycle reports without providers. |
| `just agent-test-claude-status` | Run Claude through OpenRouter and verify Gardn status from the real hook. |
| `just agent-test-codex-status` | Run Codex through OpenRouter and verify Gardn status from the real hook. |
| `just agent-test-remaining-status` | Run remaining installed agents and verify status where hooks exist. |
| `just agent-test-cursor-proxy-status` | Run Cursor through a local OpenRouter proxy and assert real hook states. |
| `just agent-test-qoder-proxy-status` | Run Qoder through a local OpenRouter proxy and assert real hook states. |

**Demo and capture**

| Recipe | What it does |
| --- | --- |
| `just demo` | Rebuild the isolated demo session on `gardn-dev`. |
| `just demo-reset` | Rebuild the demo session from a clean isolated home. |
| `just demo-window` | Open a dedicated Ghostty window. Appearance follows macOS unless `--theme` is set. |
| `just demo-window-day` | Open the capture window in Gardn Day. |
| `just demo-window-night` | Open the capture window in Gardn Night. |
| `just demo-attach` | Attach a client to the isolated demo session in the current terminal. |
| `just demo-status` | Show isolated demo session status. |
| `just demo-capture-deps` | Check Cap, cliclick, Ghostty, and gardn capture dependencies. |
| `just demo-capture` | Capture marketing shots with Cap CLI and cliclick. |
| `just demo-capture-day SHOT` | Capture one named shot in day theme. |


## Binaries

Keep these binaries on the machine:

| Binary | Source | Config and logs |
| --- | --- | --- |
| `gardn` | Latest stable GitHub release | `~/.config/gardn/` |
| `gardn-beta` | Latest GitHub prerelease `vX.Y.Z-beta.N` | `~/.config/gardn/` |
| `gardn-dev` | Current checkout | `~/.config/gardn-dev/` |

`gardn` must stay on the latest stable release. Do not install a source build over `~/.local/bin/gardn`. Use `gardn update` or the GitHub release assets when the production binary needs a new version.

Install a beta Direct Install next to it:

```bash
install -m 755 gardn-macos-aarch64 ~/.local/bin/gardn-beta
```

`gardn` and `gardn-beta` are both official release builds, so they share `~/.config/gardn`. Only one of those servers may run at a time. Stop the running server before launching the other binary. A beta client will not attach to a stable server.

`gardn-dev` is the daily development binary. Install it from this checkout:

```bash
just install-local
```

`just install-dev` is the same command. The installer writes `~/.local/bin/gardn-dev`, signs it on macOS, installs matching Linux workers under the `gardn-dev` data directory, and stops the running `gardn-dev` server. It does not touch `gardn` or `gardn-beta`.

A debug source build uses the `gardn-dev` application directory. Official release builds use `gardn`. The development namespace does not share sockets, logs, or session files with official installs.


Demo recipes use `gardn-dev --session demo` and an isolated home at `/tmp/gardn-demo` (`GARDN_DEMO_HOME` overrides). They do not touch production `gardn` or `~/.config/gardn`.

## Copy a release session into gardn-dev

`gardn-dev` starts with its own empty or leftover session. To debug against the workspaces you actually use, copy the persisted release session:

```bash
just copy-session-to-dev
```

The script stops `gardn-dev`, then copies these files from `~/.config/gardn/` into `~/.config/gardn-dev/`:

- `session.json`
- `session-history.json`
- `config.toml`
- `plugins.json`
- `ssh-profiles.json`
- `sessions/<name>/session.json` and `sessions/<name>/session-history.json`

It does not copy sockets, logs, lock files, installation IDs, or release notes. It does not stop production `gardn`.

Preview the copy:

```bash
python3 scripts/copy_release_session_to_dev.py --dry-run
```

Then launch `gardn-dev`. The copied snapshot restores groups, spaces, tabs, and pane layout. Live pane processes stay with the release server. They do not move.

## Copy a gardn-dev session back onto official gardn

If a daily session was running under `gardn-dev` (`~/.config/gardn-dev/`), restore it onto official `gardn` after a release.

1. Copy the same files listed above from `~/.config/gardn-dev/` into a backup directory.
2. Install the GitHub release binary over `~/.local/bin/gardn` only. Do not replace it with a source build.
3. Stop the `gardn-dev` namespaced daily server. Leave an isolated demo server alone.
4. Copy the backup files into `~/.config/gardn/`. Do not copy sockets, logs, lock files, or installation IDs.
5. Launch `gardn`. Agents resume from saved session refs as new processes. Live shells from the old server do not move.

`just copy-session-to-dev` only copies release to dev. The reverse path is manual.

## Loop timing overlay

Use `GARDN_DEBUG_LOOP` to diagnose UI hitches. This is a session environment flag. It is not a setting and it is not part of `config.toml`.

```bash
GARDN_DEBUG_LOOP=1 gardn-dev
```

Accepted values: `1`, `true`, `yes`, `on`.

Set the variable on both the client and the server. The overlay belongs to the process that paints. Click, hover, and typing stalls usually come from the server loop.

When the flag is on:

- A one-line HUD appears at the bottom of the window.
- The HUD reports `loop`, `drain`, `sch`, `draw`, `in`, `max`, and the last event name.
- Turns with 32ms or more of work emit a `slow ui loop` tracing warning.

`sch` is scheduled work on that loop. `draw` is paint. `drain` is queued events already in hand. `in` is input handling after the wait.

Read the logs for the process that stalled:

- `~/.config/gardn-dev/gardn-server.log`
- `~/.config/gardn-dev/gardn-client.log`

Release builds use `~/.config/gardn/` instead of `gardn-dev`.

Leave the flag unset unless you are chasing a hitch.

## Debugger builds

Routine development and test builds use limited debug information. When LLDB needs full variable and type information, use the debugging profile so the normal incremental target stays warm:

```bash
cargo build --profile debugging --package=gardn --locked
./target/debugging/gardn
```

`just install-local` installs that debugging binary as `gardn-dev`.
