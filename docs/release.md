# Release process

Hako is pre-public. Old GitHub releases are internal artifacts; do not preserve their changelog shape.

## Pending changes

Use Tegami changelog files in `.tegami/`.

```md
---
packages:
  hako: patch
---

### Fix sidebar focus

The sidebar keeps the selected workspace visible after filtering.
```

Rules:

- Add a `.tegami/*.md` file for user-facing app, docs, Nix, or website changes.
- Skip `.tegami/` for pure tests, refactors, or internal chores.
- Every release-worthy changefile must target `hako`; `just release` rejects pending changefiles that do not.
- Also target `hako-docs` for docs and `hako-nix` for Nix packaging so their package changelogs stay surface-specific.
- Keep prose user-facing. No implementation notes.
- Do not edit package `CHANGELOG.md` files by hand.

## Local workflow

```bash
pnpm install
pnpm tegami
cargo test --manifest-path apps/hako/Cargo.toml --locked --bin hako <focused-test>
git add .
git commit -m "short imperative summary"
```

Direct commits to `master` are OK while Hako is solo/pre-public. Use PRs for upstream syncs, risky release changes, or when CI/review is useful. Never force-push `origin/master`.

## Release

```bash
just release
```

`just release`:

1. requires a clean tree
2. verifies every pending Tegami changefile includes `hako`
3. runs `CI=true pnpm tegami version`
4. runs `just check`
5. commits Tegami's version/changelog changes
6. tags `v<version>`
7. pushes the branch and tag

The GitHub Release workflow builds binary assets from the pushed tag and uses the generated `apps/hako/CHANGELOG.md` section as the release body.
