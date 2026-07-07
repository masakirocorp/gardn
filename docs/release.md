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
- Target `hako` for the binary/app, `hako-docs` for docs, and `hako-nix` for Nix packaging.
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
2. runs `CI=true pnpm tegami version`
3. runs `just check`
4. commits Tegami's version/changelog changes
5. tags `v<version>`
6. pushes the branch and tag

The GitHub Release workflow builds binary assets from the pushed tag.
