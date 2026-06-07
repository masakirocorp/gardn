# hako

Terminal workspace manager for AI coding agents. Rust + ratatui.

## Principles

- **State is separated from runtime.** `AppState` is pure data, testable without PTYs or async. `PaneState` is separate from `PaneRuntime`. Workspace logic doesn't need real terminals.
- **Render is pure.** `compute_view()` handles geometry and mutations. `render()` takes `&AppState` and only draws. Never mutate state during render.
- **No god objects.** If a module is doing too many things, split it. `app/` is already split into state, actions, and input. Keep it that way.
- **Platform code is isolated.** OS-specific behavior lives in `src/platform/`. Core modules don't have `#[cfg(target_os)]`.
- **Detection is decoupled.** The detector reads a screen snapshot, never touches the parser or viewport state.
- **UI patterns should be reused.** Hako is a mouse-first TUI. New dialogs, onboarding, settings, and post-update flows should follow the existing UI/UX language and interaction patterns instead of inventing one-off screens. Prefer reusing existing modal/screen structure, affordances, and close actions so the app feels consistent.

## Multi-agent isolation

Read-only investigation can happen in the shared checkout.

Small linear changes are fine in the default main worktree when the working tree is clean and no unrelated implementation is in progress. Use a dedicated task worktree for bigger features, risky refactors, parallel edits, or whenever the main worktree already contains unrelated changes.

Use this layout for task worktrees:

- shared integration checkout: `../hako`
- task worktrees: `../hako-worktrees/<task-slug>`
- task branches: `<tracker-key>-<slug>` when a tracker ticket exists

When using a task worktree, do all code edits, tests, and validation inside that worktree. Commit on the task branch, fast-forward the shared checkout at `../hako` to the task branch commit, then push `origin/master` from `../hako`. Do not treat the task branch as the final landing branch.

If the current session is already inside an isolated task worktree, keep using it. Do not create nested worktrees.

After the change is integrated, remove the task worktree and delete the task branch locally and remotely.

## Long-lived fork workflow

This repo is a long-lived Masakiro product fork of `ogulcancelik/herdr`, branded and distributed as Hako.

- `origin` should point to `masakirocorp/hako`.
- `upstream` should point to `ogulcancelik/herdr`.
- Product trunk is `origin/master`.
- Do not force-push trunk or release branches.
- Feature branches start from product trunk.
- Force-push only feature branches, and only with `--force-with-lease`.
- Sync upstream through explicit `sync/upstream-YYYY-MM-DD` branches.
- Merge upstream with merge commits, not rebase or squash.
- Open upstream-sync PRs into `masakirocorp/hako:master`.
- Always verify the PR base is `masakirocorp/hako`, not upstream.
- Run `just check` before merging sync PRs.
- Daily upstream syncs should use the checked-in automation instead of hand-rolled commands:
  ```bash
  just sync-upstream
  ```
- `just sync-upstream` creates a `sync/upstream-YYYY-MM-DD` branch, fetches `origin` and `upstream`, merges `upstream/master` with a merge commit, runs the upstream-sync guard, writes a PR body, pushes the branch, and opens the PR.
- Treat upstream as signal, not authority: port behavior, not trust.
- For every upstream port, identify the invariant the change protects, check whether Hako has the same context, add or adjust tests in Hako for that invariant, and only then merge.
- Review `sync-report.md` in every upstream-sync PR. It calls out Hako-owned files, sensitive plumbing, and forbidden upstream identity/plumbing that must not be resurrected silently.
- Hako-owned files are intentionally protected in `.gitattributes` with `merge=keep-hako`: `README.md`, `AGENTS.md`, `SKILL.md`, `assets/logo.svg`, `docs/**`, and `website/**`. Do not ignore these paths during upstream syncs; review upstream changes against Hako's custom product/docs/site direction.
- If an upstream sync conflicts, resolve toward Hako product identity first, rerun `python3 scripts/guard_upstream_sync.py --base origin/master --upstream upstream/master --head HEAD`, then run `just check`.
- Upstream-sync PRs must pass PR CI before merge. After merge, watch the `master` CI run too; push a follow-up fix if trunk CI exposes a platform-only failure.

## Testing

Use `just` recipes by default for full tests and checks.

```bash
just test               # cargo nextest + maintenance script tests
just check              # formatting check + clippy + cargo nextest + maintenance script tests
```

During development, focused `cargo test --locked <test-name>` runs are fine for tight iteration. Before committing non-trivial changes, run `just check` unless Can explicitly accepts a narrower validation for that commit.

CI intentionally splits formatting, clippy, Rust tests, and maintenance tests into separate steps. Keep that shape; it makes platform hangs diagnosable. Rust tests use the `ci` nextest profile, which reports slow tests and times out hung tests.

Unit tests live next to the code (`#[cfg(test)] mod tests`). If you add behavior to `AppState` or `Workspace`, it should be testable with `AppState::test_new()` and `Workspace::test_new()` — no PTYs.

## Conventions

- Agents choose concise conventional commit messages, lowercase, no emojis. Do not ask for commit-message approval unless the user explicitly requests it.
- `docs/` and `website/` are Hako-owned. Do not reintroduce upstream Herdr docs/site content or generated website output unless explicitly requested.
- Put local PRDs, planning notes, and exploratory specs under `.prd/`; that directory is ignored and locally controlled.
- When work maps to an external tracker ticket, follow the team's tracker-linking convention for commit messages and PR descriptions. Do not assume GitHub issue references are in use.
- Rust: no `unwrap()` in production code. `tracing` for logging. `#[allow]` only with a comment explaining why.
- Don't bypass checks. If tests fail, fix them before committing.
- Don't add dependencies without a reason. Check if the existing deps cover it first.
- For user-facing behavior changes, update `docs/features.md` or explicitly call out why docs were not changed before release.

## Releases

Default release flow:

```bash
just check
just release 0.x.y
```

Hako release history is independent of upstream Herdr. Ignore inherited upstream `v*` tags; Hako's release line starts at `v0.1.0`.

`just release 0.x.y` bumps `Cargo.toml`, runs tests, commits, tags, and pushes. GitHub Actions builds the binaries after the tag is pushed, creates the GitHub release, and uploads all four binary assets.

After cutting a release, wait for GitHub CI, Nix, and Release workflows to pass. Verify the GitHub release exists and contains all expected assets.

The release workflow must publish these four assets:

- `hako-linux-x86_64`
- `hako-linux-aarch64`
- `hako-macos-x86_64`
- `hako-macos-aarch64`

When updating local binaries, build release and debug binaries, copy them to `~/.local/bin/hako` and `~/.local/bin/hako-dev`, codesign both on macOS, and stop the `hako-dev` server so the next launch uses the new binary. Run `cargo clean` after installing local binaries to avoid accumulating large debug build artifacts.

When changing the server/client wire protocol, compare `src/server/protocol.rs::PROTOCOL_VERSION` against the latest released tag. Bump it only if the current source protocol is not already greater than the latest released protocol. Multiple unreleased wire changes in the same release cycle must share the same single protocol bump; Hako supports tagged releases, not arbitrary `master` client/server compatibility. When a bump is required, update all hardcoded protocol expectations and manual protocol fixtures in tests. Keep protocol test expectations intentionally explicit so compatibility changes are reviewed instead of silently following the constant.


