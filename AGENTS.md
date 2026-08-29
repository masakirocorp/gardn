# Gardn

Terminal workspace manager for AI coding agents. Rust + ratatui.

## Principles

- **State is separated from runtime.** `AppState` is pure data, testable without PTYs or async. `PaneState` is separate from `PaneRuntime`. Workspace logic doesn't need real terminals.
- **Render is pure.** `compute_view()` handles geometry and mutations. `render()` takes `&AppState` and only draws. Never mutate state during render.
- **No god objects.** If a module is doing too many things, split it. `app/` is already split into state, actions, and input. Keep it that way.
- **Platform code is isolated.** OS-specific behavior lives in `apps/gardn/src/platform/`. Core modules don't have `#[cfg(target_os)]`.
- **Detection is decoupled.** The detector reads a screen snapshot, never touches the parser or viewport state.
- **UI patterns should be reused.** Gardn is a mouse-first TUI. New dialogs, onboarding, settings, and post-update flows should follow the existing UI/UX language and interaction patterns instead of inventing one-off screens. Prefer reusing existing modal/screen structure, affordances, and close actions so the app feels consistent.

## Multi-agent isolation

Read-only investigation can happen in the shared checkout.

Small linear changes are fine in the default main worktree when the working tree is clean and no unrelated implementation is in progress. Use a dedicated task worktree for bigger features, risky refactors, parallel edits, or whenever the main worktree already contains unrelated changes.

Use Worktrunk (`wt`) for task worktree create/switch/list/merge/remove. Worktrunk's configured path template owns task worktree locations.

Task branches use `<tracker-key>-<slug>` when a tracker ticket exists.

When using a task worktree, do all code edits, tests, and validation inside that worktree. Commit on the task branch, land the final commit(s) on `origin/master` through an equivalent `wt merge` flow, and do not treat the task branch as the final landing branch.

If the current session is already inside an isolated task worktree, keep using it. Do not create nested worktrees.

After the change is integrated, remove the task worktree and delete the task branch locally and remotely with `wt remove`.

## Long-lived fork workflow

This repository is a long-lived Masakiro product fork of `ogulcancelik/herdr`. It is branded and distributed as Gardn.

- `origin` should point to `masakirocorp/gardn`.
- `upstream` should point to `ogulcancelik/herdr`.
- Product trunk is `origin/master`.
- Do not force-push trunk or release branches unless the repository or organization owner explicitly authorizes one specific history rewrite in the active conversation. Authorization does not carry forward to later rewrites.
- Feature branches start from product trunk.
- For any authorized force-push, inspect the commits being replaced and use `--force-with-lease`, never `--force`.
- Sync upstream through explicit `sync/upstream-YYYY-MM-DD` branches.
- Merge upstream with merge commits, not rebase or squash.
- Open upstream-sync PRs into `masakirocorp/gardn:master`.
- Always verify the PR base is `masakirocorp/gardn`, not upstream.
- Run `pnpm check` before merging sync PRs.
- Daily upstream syncs should use the checked-in automation instead of hand-rolled commands:
  ```bash
  just sync-upstream
  ```
- `just sync-upstream` creates a `sync/upstream-YYYY-MM-DD` branch, fetches `origin` and `upstream`, merges `upstream/master` with a merge commit, runs the upstream-sync guard, writes a PR body, pushes the branch, and opens the PR.
- Treat upstream as signal, not authority: port behavior, not trust.
- For every upstream port, identify the invariant the change protects, check whether Gardn has the same context, add or adjust tests in Gardn for that invariant, and only then merge.
- Review `sync-report.md` in every upstream-sync PR. It calls out Gardn-owned files, sensitive plumbing, and forbidden upstream identity/plumbing that must not be resurrected silently.
- Gardn-owned files are intentionally protected in `.gitattributes` with `merge=keep-gardn`: `README.md`, `AGENTS.md`, `SKILL.md`, `apps/gardn/assets/logo.svg`, `docs/**`, and `website/**`. Do not ignore these paths during upstream syncs; review upstream changes against Gardn's custom product/docs/site direction.
- If an upstream sync conflicts, resolve toward Gardn product identity first, rerun `python3 scripts/guard_upstream_sync.py --base origin/master --upstream upstream/master --head HEAD`, then run `pnpm check`.
- Upstream-sync PRs must pass PR CI before merge. After merge, watch the `master` CI run too; push a follow-up fix if trunk CI exposes a platform-only failure.

## Testing

Turborepo is the canonical repository task graph. Use root pnpm scripts for routine tests and checks:

```bash
pnpm test  # local incremental Rust, maintenance, and website tests
pnpm check # complete non-incremental Rust and website quality graph
```

During development, focused `cargo test --locked <test-name>` runs are fine for tight iteration. Turbo also forwards nextest filters with `pnpm turbo run gardn#test -- <filter>`. Before committing non-trivial changes, run `pnpm check` unless Can explicitly accepts a narrower validation for that commit.

### Interactive development loop

When the user is actively iterating, prefer the shortest verification that can catch the specific mistake just introduced.

Do not run long gates (`pnpm check`, full `pnpm test`, broad test suites, release builds) during the iteration loop unless the user explicitly asks or the change is about to be committed, merged, or released.

For small UI or behavior tweaks, make the edit, run formatting/build only if needed to produce a usable `gardn-dev`, and let the user manually review. For logic/state changes, run one focused test that covers the changed behavior.

Batch small follow-up fixes before revalidating. Full checks belong at commit, merge, and release boundaries, not after every edit.

For Rust-only edit/build/run iteration, use Cargo directly:

```bash
cargo build --package=gardn --locked
./target/debug/gardn
```

Routine development and test builds use limited debug information for faster compilation. When
full LLDB variable and type information is required, use the separate debugging profile so the
normal incremental target stays warm:

```bash
cargo build --profile debugging --package=gardn --locked
./target/debugging/gardn
```

Local binaries, the `gardn` vs `gardn-dev` namespaces, session copy, and the loop
timing overlay are documented in [`docs/development.md`](docs/development.md).
`gardn` stays on the latest GitHub release. Source installs write `gardn-dev` only.

Keep incremental compilation enabled and keep each worktree's `target/` directory isolated. Do not
use a shared Cargo target, set `CARGO_INCREMENTAL=0`, add always-on sccache, or enable Turbo caching
for the native `gardn` binary as part of the normal local loop. Use pnpm/Turbo for repository-wide or
mixed Rust/website tasks, not as a wrapper around each Rust-only rebuild.

CI intentionally keeps formatting, clippy, Rust tests, structural guardrails, and maintenance tests in separate Turbo steps. Keep that shape; it makes platform hangs diagnosable. Rust tests use the `gardn#ci:test` Turbo task with non-incremental compilation in CI plus the `ci` nextest profile, which reports slow tests and times out hung tests.

Unit tests live next to the code (`#[cfg(test)] mod tests`). If you add behavior to `AppState` or `Workspace`, it should be testable with `AppState::test_new()` and `Workspace::test_new()` — no PTYs.

### Test design policy

Tests are behavior specs, not implementation snapshots. Prefer the public/user-visible seam first: rendered UI plus real mouse/key input for interaction behavior, `restore(...)` for restore behavior, public framing helpers for wire behavior, and filesystem/git/process boundaries for integration behavior.

- Keep tests flat and self-contained. Setup should be visible in the test body or a plain helper with no hidden mutable state.
- Assert the behavior that would break for users. Avoid private row indexes, helper-derived expected copy, call counts, or exact geometry unless the geometry itself is the contract.
- Render tests must assert visible output, styling, or hit behavior. A no-panic render smoke test is not enough.
- Protocol compatibility tests must pin explicit framed bytes for representative messages. Roundtrips alone do not protect compatibility.
- Process/socket tests must wait for readiness, not just path existence. Use `tests::support::connect_unix_socket` for Unix socket clients.
- Tests that mutate global environment variables must serialize that mutation and restore through `crate::config::TestEnvVar` or an equivalent RAII guard.
- Error-path tests should assert concrete error variants or useful message details, not only `is_err()`.
- Mechanical guardrails live in `scripts/test_testing_guidelines.py` and run from `pnpm test`/`pnpm check`. If a test needs an exception, prefer improving the helper or naming the invariant explicitly over weakening the guardrail.

## Conventions

- Agents choose concise conventional commit messages, lowercase, no emojis. Do not ask for commit-message approval unless the user explicitly requests it.
- `docs/` and `website/` are Gardn-owned. Do not reintroduce upstream docs, site content, or generated website output unless explicitly requested.
- Put local PRDs, planning notes, and exploratory specs under `.prd/`; that directory is ignored and locally controlled.
- When work maps to an external tracker ticket, follow the team's tracker-linking convention for commit messages and PR descriptions. Do not assume GitHub issue references are in use.
- Rust: no `unwrap()` in production code. `tracing` for logging. `#[allow]` only with a comment explaining why.
- Don't bypass checks. If tests fail, fix them before committing.
- Don't add dependencies without a reason. Check if the existing deps cover it first.
- For user-facing behavior changes, update `docs/features.md` or explicitly call out why docs were not changed before release.

## Linear workflow

Use Linear, not GitHub issues, for tracked work. The Engineering team workflow is:

- `Triage` — needs shaping, clarification, or human judgment before it enters the normal queue.
- `Backlog` — accepted work that is not pullable yet, usually blocked or deferred.
- `Ready` — fully briefed, unblocked work an agent or human can start.
- `Doing` — actively owned.
- `Review` — output exists and needs human review, approval, merge, or acceptance.
- `Done`, `Canceled`, `Duplicate` — terminal states.

Agents may pick up `Ready` issues only when they are unblocked and have clear acceptance criteria and verification. Issues in `Triage` or `Review` need human attention. Use Linear labels for stable taxonomy such as `app:gardn` and `kind:adr`, not for execution state.

## ADRs

Architecture Decision Records live in `docs/adr/` and use sequential filenames like `0001-short-slug.md`. Linear tracks ADR workflow; the repository is the source of truth for ADR content. Retroactive ADRs should record current architectural decisions that still matter, distinguish observed facts from `[INFERENCE]`, and avoid inventing historical rationale.

Use ADRs for architectural decisions that are hard to reverse, cross module or workflow boundaries, encode a real tradeoff, and would be easy for a future maintainer or agent to simplify incorrectly from code alone. Do not write ADRs for local helper design, ordinary implementation details, broad test coverage notes, or behavior already covered by an existing ADR.

Track ADR backfill and new ADR work in Linear with `kind:adr` and `app:gardn`. When adding an ADR, read the relevant source and existing ADRs first, update `docs/adr/README.md`, add durable domain terms to `CONTEXT.md` only when they help future ADR/repo reasoning, and prefer a concise record of the invariant and tradeoff over historical storytelling.

## Releases

Default release flow:

```bash
pnpm check
just release
```

Gardn is pre-public. Old GitHub releases are internal artifacts; do not preserve their changelog shape. Tegami owns version/changelog drafting from `.tegami/*.md`; see `docs/release.md`.

`just release` runs Tegami versioning, runs tests, commits, tags, and pushes. GitHub Actions builds the binaries after the tag is pushed, creates the GitHub release, and uploads all release assets.

After cutting a release, wait for GitHub CI, Nix, and Release workflows to pass. Verify the GitHub release exists and contains all expected assets.

The release workflow must publish these five assets:

- `gardn-linux-x86_64`
- `gardn-linux-aarch64`
- `gardn-macos-x86_64`
- `gardn-macos-aarch64`
- `gardn-windows-x86_64.exe`

When updating the local development binary, run `just install-local`. That installs `~/.local/bin/gardn-dev` only. Do not install a source build over `~/.local/bin/gardn`. Production `gardn` stays on the latest GitHub release. After install, the next `gardn-dev` launch uses the new binary. `just install-local` runs `cargo clean` after install to avoid accumulating large debug build artifacts.

When changing the server/client wire protocol, compare `apps/gardn/src/protocol/wire.rs::PROTOCOL_VERSION` against the latest Gardn release tag. Bump it only if the current source protocol is not already greater than the latest released protocol. Multiple unreleased wire changes in the same release cycle must share the same single protocol bump; Gardn supports tagged releases, not arbitrary `master` client/server compatibility. When a bump is required, update all hardcoded protocol expectations and manual protocol fixtures in tests. Keep protocol test expectations intentionally explicit so compatibility changes are reviewed instead of silently following the constant.


