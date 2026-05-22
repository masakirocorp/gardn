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

Small changes or small tasks are fine in the default main worktree. If you find unrelated implementation changes already in progress in the main worktree, use a dedicated worktree instead. Use a dedicated worktree for bigger features too.

Use this layout:

- shared integration checkout: `../hako`
- task worktrees: `../hako-worktrees/<task-slug>`
- task branches: `issue/<id>-<slug>` when an issue exists

Do all code edits, tests, and validation inside the task worktree.

Commit on the task branch in that worktree.

When the change is ready, fast-forward the shared checkout at `../hako` to the task branch commit, then push `origin/master` from `../hako`. Do not treat the task branch as the final landing branch.

If the current session is already inside an isolated task worktree, keep using it. Do not create nested worktrees.

Before committing, propose the commit message and get alignment.

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

## Testing

Use `just` recipes by default for tests and checks instead of invoking cargo or scripts directly.

```bash
just test               # cargo nextest + maintenance script tests
just check              # formatting check + cargo nextest + maintenance script tests
```

Default flow: run `just check` before committing. Do not commit until `just check` passes locally unless Can explicitly accepts a narrower validation for that commit.

Unit tests live next to the code (`#[cfg(test)] mod tests`). If you add behavior to `AppState` or `Workspace`, it should be testable with `AppState::test_new()` and `Workspace::test_new()` — no PTYs.

## Conventions

- Conventional commits, lowercase, no emojis.
- The marketing website is hosted outside this repository. Do not add website assets, screenshots, or generated site output here unless explicitly requested.
- Put local PRDs, planning notes, and exploratory specs under `.prd/`; that directory is ignored and locally controlled.
- When a normal feature or fix commit relates to a GitHub issue, add a commit body line `refs #<issue-number>` after the subject. Use this shape:
  ```text
  fix: handle pane focus

  refs #82
  ```
  Do not use GitHub closing keywords like `fixes #<issue-number>`, `closes #<issue-number>`, or `resolves #<issue-number>` in normal commits unless you intentionally want GitHub to close the issue when the commit lands on the default branch.
- Rust: no `unwrap()` in production code. `tracing` for logging. `#[allow]` only with a comment explaining why.
- Don't bypass checks. If tests fail, fix them before committing.
- Don't add dependencies without a reason. Check if the existing deps cover it first.

## Releases

Before cutting the first public Hako release, define the release notes flow. The current release recipe only bumps the version, runs checks, commits, tags, and lets GitHub Actions build release artifacts.

Default release flow:

```bash
just check
just release 0.x.y
```

`just release 0.x.y` bumps `Cargo.toml`, runs tests, commits, tags, and pushes. GitHub Actions builds the binaries after the tag is pushed, creates the GitHub release, and uploads all four binary assets.

The release workflow must publish these four assets:

- `hako-linux-x86_64`
- `hako-linux-aarch64`
- `hako-macos-x86_64`
- `hako-macos-aarch64`


When changing the server/client wire protocol, compare `src/server/protocol.rs::PROTOCOL_VERSION` against the latest released tag. Bump it only if the current source protocol is not already greater than the latest released protocol. Multiple unreleased wire changes in the same release cycle must share the same single protocol bump; Hako supports tagged releases, not arbitrary `master` client/server compatibility. When a bump is required, update all hardcoded protocol expectations and manual protocol fixtures in tests. Keep protocol test expectations intentionally explicit so compatibility changes are reviewed instead of silently following the constant.

