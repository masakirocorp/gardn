# Release process

Oh My Herdr is pre-public. Old GitHub releases are internal artifacts; do not preserve their changelog shape.

## Pending changes

Use Tegami changelog files in `.tegami/`.

```md
---
packages:
  omh: patch
---

### Fix sidebar focus

The sidebar keeps the selected workspace visible after filtering.
```

Rules:

- Add a `.tegami/*.md` file for user-facing app, docs, Nix, or website changes.
- Skip `.tegami/` for pure tests, refactors, or internal chores.
- Every release-worthy changefile must target `omh`; `just release` rejects pending changefiles that do not.
- Also target `omh-docs` for docs and `omh-nix` for Nix packaging so their package changelogs stay surface-specific.
- Keep prose user-facing. No implementation notes.
- Do not edit package `CHANGELOG.md` files by hand.

## Local workflow

```bash
pnpm install
pnpm tegami
cargo test --manifest-path apps/omh/Cargo.toml --locked --bin omh <focused-test>
git add .
git commit -m "short imperative summary"
```

Direct commits to `master` are OK while Oh My Herdr is solo/pre-public. Use PRs for upstream syncs, risky release changes, or when CI/review is useful. Never force-push `origin/master`.

## Manual QA

Use the [manual QA matrix](manual-qa.md) to select checks during development. Before tagging a release, run M01-M08 and the P1 cases affected by the release. After the release artifacts publish, run M09 before treating the release as cleared.

## Release

```bash
just release
```

`just release`:

1. requires a clean tree
2. verifies every pending Tegami changefile includes `omh`
3. runs `CI=true pnpm tegami version`
4. runs `just check`
5. commits Tegami's version/changelog changes
6. tags `v<version>`
7. pushes the branch and tag

The GitHub Release workflow builds binary assets from the pushed tag and uses the generated `apps/omh/CHANGELOG.md` section as the release body.
